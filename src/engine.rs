use crate::state::{CropArea, EditState};
use image::{imageops::FilterType, RgbaImage};
use rayon::prelude::*;

pub struct PipelineEngine;

impl PipelineEngine {
    /// Cắt ảnh theo vùng chọn (ROI)
    pub fn apply_crop(img: &RgbaImage, crop: &Option<CropArea>) -> RgbaImage {
        match crop {
            Some(area) => {
                if area.x >= img.width() || area.y >= img.height() {
                    return img.clone();
                }
                let max_x = area.x.saturating_add(area.width).min(img.width());
                let max_y = area.y.saturating_add(area.height).min(img.height());
                let w = max_x.saturating_sub(area.x);
                let h = max_y.saturating_sub(area.y);
                if w == 0 || h == 0 {
                    return img.clone();
                }
                image::imageops::crop_imm(img, area.x, area.y, w, h).to_image()
            }
            None => img.clone(),
        }
    }

    /// Tạo preview 1080p từ ảnh gốc để xem trước mà không lag máy
    pub fn create_preview_buffer(img: &RgbaImage, max_edge: u32) -> RgbaImage {
        let (w, h) = (img.width(), img.height());
        if max_edge == 0 || w == 0 || h == 0 || (w <= max_edge && h <= max_edge) {
            return img.clone();
        }
        let (new_w, new_h) = if w > h {
            (
                max_edge,
                ((h as u64 * max_edge as u64) / w as u64).max(1) as u32,
            )
        } else {
            (
                ((w as u64 * max_edge as u64) / h as u64).max(1) as u32,
                max_edge,
            )
        };
        image::imageops::resize(img, new_w, new_h, FilterType::Triangle)
    }

    fn srgb_to_linear(value: f32) -> f32 {
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }

    fn linear_to_srgb(value: f32) -> f32 {
        if value <= 0.0031308 {
            value * 12.92
        } else {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        }
    }

    fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let hue = if delta == 0.0 {
            0.0
        } else if max == r {
            ((g - b) / delta).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / delta + 2.0) / 6.0
        } else {
            ((r - g) / delta + 4.0) / 6.0
        };
        let saturation = if max == 0.0 { 0.0 } else { delta / max };
        (hue, saturation, max)
    }

    fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> (f32, f32, f32) {
        if saturation == 0.0 {
            return (value, value, value);
        }
        let h = hue.rem_euclid(1.0) * 6.0;
        let sector = h.floor() as u32;
        let fraction = h - sector as f32;
        let p = value * (1.0 - saturation);
        let q = value * (1.0 - saturation * fraction);
        let t = value * (1.0 - saturation * (1.0 - fraction));
        match sector {
            0 => (value, t, p),
            1 => (q, value, p),
            2 => (p, value, t),
            3 => (p, q, value),
            4 => (t, p, value),
            _ => (value, p, q),
        }
    }

    /// Applies the documented linear-light -> sRGB -> HSV color pipeline.
    pub fn process_pixels(buffer: &mut RgbaImage, state: &EditState) {
        if buffer.width() == 0 || buffer.height() == 0 {
            return;
        }
        let exp_mult = 2.0_f32.powf(state.exposure);
        let temperature = state.temperature.clamp(-1.0, 1.0);
        let warm = temperature.max(0.0);
        let cool = (-temperature).max(0.0);
        let red_gain = 1.0 + warm * 0.3 - cool * 0.2;
        let blue_gain = 1.0 - warm * 0.2 + cool * 0.3;
        let contrast_factor = 1.0 + state.contrast.clamp(-1.0, 1.0);
        let brightness = state.brightness.clamp(-1.0, 1.0);
        let saturation = state.saturation.clamp(-1.0, 1.0);
        let vibrance = state.vibrance.clamp(-1.0, 1.0);
        let stride = buffer.width() as usize * 4;

        buffer.par_chunks_exact_mut(stride).for_each(|row| {
            for pixel in row.chunks_exact_mut(4) {
                let mut r = Self::srgb_to_linear(pixel[0] as f32 / 255.0) * red_gain * exp_mult;
                let mut g = Self::srgb_to_linear(pixel[1] as f32 / 255.0) * exp_mult;
                let mut b = Self::srgb_to_linear(pixel[2] as f32 / 255.0) * blue_gain * exp_mult;

                r = Self::linear_to_srgb(r);
                g = Self::linear_to_srgb(g);
                b = Self::linear_to_srgb(b);
                r = (r - 0.5) * contrast_factor + 0.5 + brightness;
                g = (g - 0.5) * contrast_factor + 0.5 + brightness;
                b = (b - 0.5) * contrast_factor + 0.5 + brightness;

                let (h, base_saturation, value) = Self::rgb_to_hsv(r, g, b);
                let vibrance_scale = 1.0 + vibrance * (1.0 - base_saturation);
                let final_saturation =
                    (base_saturation * (1.0 + saturation) * vibrance_scale).clamp(0.0, 1.0);
                (r, g, b) = Self::hsv_to_rgb(h, final_saturation, value);

                pixel[0] = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
                pixel[1] = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
                pixel[2] = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        });
    }

    /// Rotates an image around its center using inverse mapping and bilinear sampling.
    pub fn rotate(source: &RgbaImage, angle_degrees: f32) -> RgbaImage {
        let mut output = RgbaImage::new(source.width(), source.height());
        let angle = angle_degrees.to_radians();
        let (sin, cos) = angle.sin_cos();
        let source_center = (
            (source.width() as f32 - 1.0) / 2.0,
            (source.height() as f32 - 1.0) / 2.0,
        );
        let output_center = (
            (output.width() as f32 - 1.0) / 2.0,
            (output.height() as f32 - 1.0) / 2.0,
        );

        for (x, y, pixel) in output.enumerate_pixels_mut() {
            let dx = x as f32 - output_center.0;
            let dy = y as f32 - output_center.1;
            let source_x = cos * dx + sin * dy + source_center.0;
            let source_y = -sin * dx + cos * dy + source_center.1;
            if source_x < 0.0
                || source_y < 0.0
                || source_x > source.width() as f32 - 1.0
                || source_y > source.height() as f32 - 1.0
            {
                *pixel = image::Rgba([0, 0, 0, 0]);
                continue;
            }
            *pixel = Self::bilinear_sample(source, source_x, source_y);
        }
        output
    }

    fn bilinear_sample(source: &RgbaImage, x: f32, y: f32) -> image::Rgba<u8> {
        let x0 = x.floor() as u32;
        let y0 = y.floor() as u32;
        let x1 = (x0 + 1).min(source.width() - 1);
        let y1 = (y0 + 1).min(source.height() - 1);
        let dx = x - x0 as f32;
        let dy = y - y0 as f32;
        let top_left = source.get_pixel(x0, y0);
        let top_right = source.get_pixel(x1, y0);
        let bottom_left = source.get_pixel(x0, y1);
        let bottom_right = source.get_pixel(x1, y1);
        let mut result = [0_u8; 4];
        for channel in 0..4 {
            let top = top_left[channel] as f32 * (1.0 - dx) + top_right[channel] as f32 * dx;
            let bottom =
                bottom_left[channel] as f32 * (1.0 - dx) + bottom_right[channel] as f32 * dx;
            result[channel] = (top * (1.0 - dy) + bottom * dy).round() as u8;
        }
        image::Rgba(result)
    }

    /// Draws a continuous freehand segment with Bresenham's integer line algorithm.
    pub fn draw_line(
        buffer: &mut RgbaImage,
        start: (i32, i32),
        end: (i32, i32),
        color: image::Rgba<u8>,
        thickness: u32,
    ) {
        let mut x = start.0;
        let mut y = start.1;
        let dx = (end.0 - start.0).abs();
        let sx = if start.0 < end.0 { 1 } else { -1 };
        let dy = -(end.1 - start.1).abs();
        let sy = if start.1 < end.1 { 1 } else { -1 };
        let mut error = dx + dy;
        let radius = (thickness.max(1) as i32 - 1) / 2;

        loop {
            Self::draw_brush(buffer, x, y, radius, color);
            if x == end.0 && y == end.1 {
                break;
            }
            let double_error = 2 * error;
            if double_error >= dy {
                error += dy;
                x += sx;
            }
            if double_error <= dx {
                error += dx;
                y += sy;
            }
        }
    }

    fn draw_brush(buffer: &mut RgbaImage, x: i32, y: i32, radius: i32, color: image::Rgba<u8>) {
        for brush_y in (y - radius)..=(y + radius) {
            for brush_x in (x - radius)..=(x + radius) {
                if brush_x >= 0
                    && brush_y >= 0
                    && (brush_x as u32) < buffer.width()
                    && (brush_y as u32) < buffer.height()
                {
                    buffer.put_pixel(brush_x as u32, brush_y as u32, color);
                }
            }
        }
    }

    /// Composites a foreground RGBA layer over the background with Porter-Duff "over".
    pub fn blend_overlay(background: &mut RgbaImage, foreground: &RgbaImage, x: i32, y: i32) {
        for (foreground_x, foreground_y, source) in foreground.enumerate_pixels() {
            let destination_x = x + foreground_x as i32;
            let destination_y = y + foreground_y as i32;
            if destination_x < 0
                || destination_y < 0
                || destination_x as u32 >= background.width()
                || destination_y as u32 >= background.height()
            {
                continue;
            }
            let destination = background.get_pixel_mut(destination_x as u32, destination_y as u32);
            let source_alpha = source[3] as f32 / 255.0;
            let destination_alpha = destination[3] as f32 / 255.0;
            let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
            if output_alpha == 0.0 {
                *destination = image::Rgba([0, 0, 0, 0]);
                continue;
            }
            for channel in 0..3 {
                let value = (source[channel] as f32 * source_alpha
                    + destination[channel] as f32 * destination_alpha * (1.0 - source_alpha))
                    / output_alpha;
                destination[channel] = value.round() as u8;
            }
            destination[3] = (output_alpha * 255.0).round() as u8;
        }
    }

    /// Render hoàn chỉnh trên một buffer ảnh
    pub fn render(source: &RgbaImage, state: &EditState) -> RgbaImage {
        let cropped = Self::apply_crop(source, &state.crop);
        let mut rotated = Self::rotate(&cropped, state.rotation_degrees);
        Self::process_pixels(&mut rotated, state);
        for stroke in &state.strokes {
            for segment in stroke.points.windows(2) {
                Self::draw_line(
                    &mut rotated,
                    segment[0],
                    segment[1],
                    image::Rgba(stroke.color),
                    stroke.thickness,
                );
            }
        }
        for sticker in &state.stickers {
            let mut layer = RgbaImage::new(sticker.width, sticker.height);
            for pixel in layer.pixels_mut() {
                *pixel = image::Rgba(sticker.color);
            }
            Self::blend_overlay(&mut rotated, &layer, sticker.x, sticker.y);
        }
        rotated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{FreehandStroke, Sticker};

    #[test]
    fn crop_clamps_and_rejects_out_of_bounds_regions() {
        let image = RgbaImage::from_pixel(4, 3, image::Rgba([1, 2, 3, 255]));
        let cropped = PipelineEngine::apply_crop(
            &image,
            &Some(CropArea {
                x: 2,
                y: 1,
                width: 20,
                height: 20,
            }),
        );
        assert_eq!((cropped.width(), cropped.height()), (2, 2));

        let unchanged = PipelineEngine::apply_crop(
            &image,
            &Some(CropArea {
                x: 10,
                y: 0,
                width: 1,
                height: 1,
            }),
        );
        assert_eq!((unchanged.width(), unchanged.height()), (4, 3));
    }

    #[test]
    fn neutral_render_preserves_pixels() {
        let image = RgbaImage::from_pixel(1, 1, image::Rgba([64, 128, 192, 37]));
        let rendered = PipelineEngine::render(&image, &EditState::default());
        assert_eq!(rendered.get_pixel(0, 0), image.get_pixel(0, 0));
    }

    #[test]
    fn alpha_blending_composes_transparent_and_opaque_layers() {
        let mut background = RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 255, 255]));
        let foreground = RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 128]));
        PipelineEngine::blend_overlay(&mut background, &foreground, 0, 0);
        let pixel = background.get_pixel(0, 0);
        assert!(pixel[0] > 120 && pixel[0] < 140);
        assert!(pixel[2] > 120 && pixel[2] < 140);
        assert_eq!(pixel[3], 255);
    }

    #[test]
    fn bresenham_line_reaches_both_endpoints() {
        let mut image = RgbaImage::new(5, 5);
        PipelineEngine::draw_line(&mut image, (0, 0), (4, 4), image::Rgba([255, 0, 0, 255]), 1);
        assert_eq!(image.get_pixel(0, 0)[0], 255);
        assert_eq!(image.get_pixel(4, 4)[0], 255);
    }

    #[test]
    fn rotation_preserves_canvas_size() {
        let image = RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]));
        let rotated = PipelineEngine::rotate(&image, 90.0);
        assert_eq!((rotated.width(), rotated.height()), (3, 2));
    }

    #[test]
    fn render_applies_persistent_strokes_and_stickers() {
        let image = RgbaImage::from_pixel(8, 8, image::Rgba([0, 0, 0, 255]));
        let mut state = EditState::default();
        state.strokes.push(FreehandStroke {
            points: vec![(0, 0), (3, 3)],
            color: [255, 0, 0, 255],
            thickness: 1,
        });
        state.stickers.push(Sticker {
            x: 5,
            y: 5,
            width: 2,
            height: 2,
            color: [0, 255, 0, 255],
        });

        let rendered = PipelineEngine::render(&image, &state);
        assert_eq!(rendered.get_pixel(3, 3), &image::Rgba([255, 0, 0, 255]));
        assert_eq!(rendered.get_pixel(5, 5), &image::Rgba([0, 255, 0, 255]));
    }

    #[test]
    fn edit_state_serializes_persistent_layers() {
        let mut state = EditState::default();
        state.strokes.push(FreehandStroke {
            points: vec![(1, 2), (3, 4)],
            color: [1, 2, 3, 4],
            thickness: 2,
        });
        state.stickers.push(Sticker {
            x: 4,
            y: 5,
            width: 6,
            height: 7,
            color: [8, 9, 10, 11],
        });
        let restored: EditState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(restored.strokes.len(), 1);
        assert_eq!(restored.stickers.len(), 1);
    }
}
