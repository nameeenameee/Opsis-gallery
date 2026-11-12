use crate::engine::PipelineEngine;
use crate::state::EditState;
use crate::sticker_extractor::{
    extract_fallback_region_grow, ExtractedSticker, YoloDetectionCache,
};
use image::RgbaImage;
use std::path::Path;
use std::sync::OnceLock;

pub struct EditorSession {
    pub file_path: String,
    pub original_dim: (u32, u32),
    pub proxy_source: RgbaImage,
    yolo_cache: OnceLock<Option<YoloDetectionCache>>,
}

impl EditorSession {
    pub fn new(path: &str) -> Result<Self, image::ImageError> {
        let master = image::open(Path::new(path))?.to_rgba8();
        let original_dim = master.dimensions();
        let proxy_source = PipelineEngine::create_preview_buffer(&master, 1080);

        Ok(Self {
            file_path: path.to_string(),
            original_dim,
            proxy_source,
            yolo_cache: OnceLock::new(),
        })
    }

    pub fn extract_sticker(&self, seed_u: f32, seed_v: f32) -> Option<ExtractedSticker> {
        let cache = self
            .yolo_cache
            .get_or_init(|| YoloDetectionCache::build(&self.proxy_source));

        match cache {
            Some(yolo) => yolo.extract_at(&self.proxy_source, seed_u, seed_v),
            None => extract_fallback_region_grow(&self.proxy_source, seed_u, seed_v, 40, 70),
        }
    }

    pub fn render_preview(&self, state: &EditState) -> RgbaImage {
        PipelineEngine::render(&self.proxy_source, state)
    }

    pub fn export_master(&self, state: &EditState) -> Result<RgbaImage, image::ImageError> {
        let master = image::open(Path::new(&self.file_path))?.to_rgba8();
        Ok(PipelineEngine::render(&master, state))
    }
}
