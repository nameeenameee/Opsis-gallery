use crate::engine::PipelineEngine;
use crate::state::EditState;
use image::{DynamicImage, RgbaImage};
use std::fs;
use std::path::Path;
use std::sync::mpsc::Sender;
use uuid::Uuid;

pub struct LifecycleManager;
pub type ImageLifecycleManager = LifecycleManager;

impl LifecycleManager {
    /// Loads EditState JSON from cache if present
    pub fn load_state(
        cache_dir: &Path,
        image_id: &str,
    ) -> Result<Option<EditState>, Box<dyn std::error::Error>> {
        let state_path = cache_dir.join(format!("{}.json", image_id));
        if !state_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(state_path)?;
        let state: EditState = serde_json::from_str(&content)?;
        Ok(Some(state))
    }

    /// ED-03: Save EditState JSON and export 1080p cached preview
    pub fn handle_save(
        cache_dir: &Path,
        original: &RgbaImage,
        state: &EditState,
    ) -> Result<(), Box<dyn std::error::Error>> {
        fs::create_dir_all(cache_dir)?;

        let id = if state.image_id.is_empty() {
            "default"
        } else {
            &state.image_id
        };

        // 1. Serialize EditState
        let state_path = cache_dir.join(format!("{}.json", id));
        let serialized = serde_json::to_string_pretty(state)?;
        fs::write(state_path, serialized)?;

        // 2. Export 1080p cached preview
        let preview_source = PipelineEngine::create_preview_buffer(original, 1080);
        let preview = PipelineEngine::render(&preview_source, state);
        let preview_path = cache_dir.join(format!("{}_preview.jpg", id));
        let rgb_img = DynamicImage::ImageRgba8(preview).into_rgb8();
        rgb_img.save_with_format(preview_path, image::ImageFormat::Jpeg)?;

        Ok(())
    }

    /// ED-04: Apply edits to full-res master, write copy, notify AI daemon
    /// Signature matches `src/main.rs:73`: (output_dir, original, state, ai_channel)
    pub fn handle_save_as_copy(
        output_dir: &Path,
        original: &RgbaImage,
        state: &EditState,
        ai_channel: &Sender<String>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        fs::create_dir_all(output_dir)?;

        let new_uuid = Uuid::new_v4().to_string();
        let target_path = output_dir.join(format!("img_{}.jpg", new_uuid));

        let full_res = PipelineEngine::render(original, state);
        let rgb_img = DynamicImage::ImageRgba8(full_res).into_rgb8();
        rgb_img.save_with_format(&target_path, image::ImageFormat::Jpeg)?;

        let path_str = target_path.to_string_lossy().to_string();
        let _ = ai_channel.send(path_str.clone());

        Ok(path_str)
    }

    /// ED-05: Revert edits by deleting state and preview files
    pub fn handle_revert(cache_dir: &Path, image_id: &str) -> Result<(), std::io::Error> {
        let state_path = cache_dir.join(format!("{}.json", image_id));
        let preview_path = cache_dir.join(format!("{}_preview.jpg", image_id));

        if state_path.exists() {
            fs::remove_file(state_path)?;
        }
        if preview_path.exists() {
            fs::remove_file(preview_path)?;
        }

        Ok(())
    }
}
