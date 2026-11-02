use crate::engine::PipelineEngine;
use crate::state::EditState;
use image::RgbaImage;
use std::path::Path;

pub struct EditorSession {
    pub file_path: String,
    pub original_dim: (u32, u32),
    pub proxy_source: RgbaImage,
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
        })
    }

    pub fn render_preview(&self, state: &EditState) -> RgbaImage {
        PipelineEngine::render(&self.proxy_source, state)
    }

    pub fn export_master(&self, state: &EditState) -> Result<RgbaImage, image::ImageError> {
        let master = image::open(Path::new(&self.file_path))?.to_rgba8();
        Ok(PipelineEngine::render(&master, state))
    }
}
