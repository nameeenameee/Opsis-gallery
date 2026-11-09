pub mod cli;
pub mod commands;
pub mod engine;
pub mod lifecycle;
pub mod session;
pub mod state;
pub mod sticker_extractor;

#[cfg(test)]
mod tests {
    use super::engine::PipelineEngine;
    use super::lifecycle::LifecycleManager;
    use super::state::{CropArea, EditState, FreehandStroke, Preset};
    use image::{ImageBuffer, Rgba, RgbaImage};
    use std::path::Path;
    use std::sync::mpsc;

    fn create_test_pattern(w: u32, h: u32) -> RgbaImage {
        ImageBuffer::from_fn(w, h, |x, y| {
            Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        })
    }

    #[test]
    fn test_identity_render() {
        let img = create_test_pattern(100, 100);
        let state = EditState::default();
        let rendered = PipelineEngine::render(&img, &state);

        assert_eq!(rendered.dimensions(), (100, 100));
        assert_eq!(rendered.get_pixel(50, 50), img.get_pixel(50, 50));
    }

    #[test]
    fn test_color_pipeline_precision_tolerance() {
        let mut img = create_test_pattern(10, 10);
        let original_pixel = *img.get_pixel(5, 5);
        let mut state = EditState::default();
        state.brightness = 0.0001; // Force process_pixels evaluation

        PipelineEngine::process_pixels(&mut img, &state);
        let modified_pixel = *img.get_pixel(5, 5);

        for c in 0..3 {
            let delta = (original_pixel[c] as i32 - modified_pixel[c] as i32).abs();
            assert!(delta <= 2, "Channel {} drifted by >2 LSB: {}", c, delta);
        }
    }

    #[test]
    fn test_clamped_safe_crop() {
        let img = create_test_pattern(200, 200);
        let mut state = EditState::default();
        state.crop = Some(CropArea {
            x: 150,
            y: 150,
            width: 100,
            height: 100,
        });

        let rendered = PipelineEngine::render(&img, &state);
        assert_eq!(rendered.dimensions(), (50, 50));
    }

    #[test]
    fn test_rotation_dimensions() {
        let img = create_test_pattern(100, 50);
        let mut state = EditState::default();
        state.rotation_degrees = 90.0;

        let rendered = PipelineEngine::render(&img, &state);
        assert_eq!(rendered.width(), 50);
        assert_eq!(rendered.height(), 100);
    }

    #[test]
    fn test_extract_sticker_from_seed() {
        let image = ImageBuffer::from_fn(20, 20, |x, _| {
            if x < 10 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });

        let extracted =
            super::sticker_extractor::extract_sticker_from_seed(&image, 0.25, 0.5, 40, 70)
                .expect("seed should identify a region");

        assert!(extracted.bounds_norm.2 > 0.0);
        assert!(extracted.bounds_norm.3 > 0.0);
        assert!((8..=10).contains(&extracted.image.width()));
    }

    #[test]
    fn test_color_pipeline_monochrome() {
        let img = create_test_pattern(64, 64);
        let mut state = EditState::default();
        state.preset = Preset::Monochrome;

        let rendered = PipelineEngine::render(&img, &state);
        let pixel = rendered.get_pixel(10, 10);
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
    }

    #[test]
    fn test_minimum_contrast_preserves_color() {
        let img = ImageBuffer::from_pixel(1, 1, Rgba([220, 80, 40, 255]));
        let mut state = EditState::default();
        state.contrast = 0.0;

        let rendered = PipelineEngine::render(&img, &state);
        let pixel = rendered.get_pixel(0, 0);

        assert_ne!(pixel[0], pixel[1]);
        assert_ne!(pixel[1], pixel[2]);
    }

    #[test]
    fn test_reduced_contrast_preserves_saturation() {
        let img = ImageBuffer::from_pixel(1, 1, Rgba([220, 80, 40, 255]));
        let mut state = EditState::default();
        state.contrast = 0.0;

        let rendered = PipelineEngine::render(&img, &state);
        let pixel = rendered.get_pixel(0, 0);

        assert!(pixel[0] > pixel[1]);
        assert!(pixel[1] > pixel[2]);
    }

    #[test]
    fn test_vibrance_does_not_change_neutral_pixels() {
        let img = ImageBuffer::from_pixel(1, 1, Rgba([128, 128, 128, 255]));
        let baseline = PipelineEngine::render(&img, &EditState::default());
        let mut vibrant_state = EditState::default();
        vibrant_state.vibrance = 1.0;

        let rendered = PipelineEngine::render(&img, &vibrant_state);
        assert_eq!(rendered.get_pixel(0, 0), baseline.get_pixel(0, 0));
    }

    #[test]
    fn test_freehand_stroke() {
        let img = create_test_pattern(100, 100);
        let mut state = EditState::default();
        state.strokes.push(FreehandStroke {
            points: vec![(10, 10), (90, 10)],
            color: [255, 0, 0, 255],
            thickness: 4,
        });

        let rendered = PipelineEngine::render(&img, &state);
        let pixel = rendered.get_pixel(50, 10);
        assert_eq!(pixel[0], 255);
    }

    #[test]
    fn test_lifecycle_save_and_revert() {
        let cache_dir = Path::new("target/test_cache");
        let img = create_test_pattern(64, 64);
        let mut state = EditState::default();
        state.image_id = "test_img_001".to_string();

        LifecycleManager::handle_save(cache_dir, &img, &state).unwrap();
        let loaded = LifecycleManager::load_state(cache_dir, "test_img_001")
            .unwrap()
            .expect("State should exist");
        assert_eq!(loaded.image_id, "test_img_001");

        LifecycleManager::handle_revert(cache_dir, "test_img_001").unwrap();
        let reverted = LifecycleManager::load_state(cache_dir, "test_img_001").unwrap();
        assert!(reverted.is_none());
    }

    #[test]
    fn test_save_as_copy_ai_channel() {
        let out_dir = Path::new("target/test_out");
        let img = create_test_pattern(32, 32);
        let state = EditState::default();
        let (tx, rx) = mpsc::channel();

        let path = LifecycleManager::handle_save_as_copy(out_dir, &img, &state, &tx).unwrap();
        let msg = rx
            .recv()
            .expect("Should receive notification channel message");
        assert_eq!(path, msg);
        assert!(Path::new(&path).exists());
    }
}
