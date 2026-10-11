use crate::engine::PipelineEngine;
use crate::state::EditState;
use image::{DynamicImage, RgbaImage};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct ImageLifecycleManager;

impl ImageLifecycleManager {
    pub fn load_state(
        cache_dir: &Path,
        image_id: &str,
    ) -> Result<Option<EditState>, Box<dyn std::error::Error>> {
        let state_path = cache_dir.join(format!("{image_id}.json"));
        if !state_path.exists() {
            return Ok(None);
        }
        let state = serde_json::from_str(&fs::read_to_string(state_path)?)?;
        Ok(Some(state))
    }

    /// ED-03: Bấm SAVE chuẩn
    /// - Ghi file JSON state (Edit Layer)
    /// - Xuất ảnh preview 1080p vào cache để xem nhanh
    /// - TUYỆT ĐỐI KHÔNG động vào file gốc, KHÔNG re-index vector AI
    pub fn handle_save(
        cache_dir: &Path,
        original: &RgbaImage,
        state: &EditState,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 1. Lưu JSON Edit State
        let state_path = cache_dir.join(format!("{}.json", state.image_id));
        let json = serde_json::to_string_pretty(state)?;
        fs::write(state_path, json)?;

        // 2. Xuất cached preview 1080p (Chuyển sang RGB trước khi lưu file .jpg)
        let preview_buffer = PipelineEngine::create_preview_buffer(original, 1080);
        let rendered_preview = PipelineEngine::render(&preview_buffer, state);
        let preview_path = cache_dir.join(format!("{}_preview.jpg", state.image_id));

        let rgb_preview = DynamicImage::ImageRgba8(rendered_preview).into_rgb8();
        rgb_preview.save(preview_path)?;

        println!(
            "[ED-03] Save hoàn tất: Đã lưu JSON layer & Cached Preview. Vector AI giữ nguyên!"
        );
        Ok(())
    }

    /// ED-04: Bấm SAVE AS COPY
    /// - Flatten ra file ảnh mới tinh trên đĩa
    /// - Tạo Unique ID mới
    /// - Bắn đường dẫn file mới qua Channel để AI ngầm chuẩn bị index
    pub fn handle_save_as_copy(
        output_dir: &Path,
        original: &RgbaImage,
        state: &EditState,
        ai_queue_sender: &std::sync::mpsc::Sender<String>,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let new_id = format!("img_{}", Uuid::new_v4());
        let new_file_path = output_dir.join(format!("{}.jpg", new_id));

        // Flatten ảnh gốc full-res với thông số chỉnh sửa (Chuyển sang RGB trước khi lưu file .jpg)
        let exported_image = PipelineEngine::render(original, state);
        let rgb_exported = DynamicImage::ImageRgba8(exported_image).into_rgb8();
        rgb_exported.save(&new_file_path)?;

        // Bắn ID sang queue cho AI Daemon (của Duy) index
        ai_queue_sender.send(new_file_path.to_string_lossy().to_string())?;

        println!(
            "[ED-04] Save as Copy hoàn tất: Xuất file '{}'. Đã báo AI Daemon index!",
            new_id
        );
        Ok(new_file_path)
    }

    /// ED-05: Bấm REVERT TO ORIGINAL
    /// - Xóa file JSON state
    /// - Phục hồi ảnh nguyên gốc tức thì (0ms chạy lại AI)
    pub fn handle_revert(cache_dir: &Path, image_id: &str) -> Result<(), std::io::Error> {
        let state_path = cache_dir.join(format!("{}.json", image_id));
        if state_path.exists() {
            fs::remove_file(state_path)?;
        }
        println!("[ED-05] Revert hoàn tất: Xóa Edit Layer, khôi phục ảnh nguyên gốc!");
        Ok(())
    }
}
