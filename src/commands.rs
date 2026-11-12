#[cfg(feature = "tauri-ipc")]
pub mod tauri_bindings {
    use crate::session::EditorSession;
    use crate::state::EditState;
    use arboard::{Clipboard, ImageData};
    use image::{ImageBuffer, Rgba};
    use serde::Serialize;
    use std::borrow::Cow;
    use std::fs;
    use std::sync::Mutex;
    use tauri::State;

    pub struct AppState {
        pub editor: Mutex<Option<EditorSession>>,
    }

    #[tauri::command]
    pub fn init_editor(path: String, state: State<'_, AppState>) -> Result<(u32, u32), String> {
        let session = EditorSession::new(&path).map_err(|e| e.to_string())?;
        let dims = session.proxy_source.dimensions();

        let mut lock = state.editor.lock().map_err(|e| e.to_string())?;
        *lock = Some(session);

        Ok(dims)
    }

    #[tauri::command]
    pub fn render_live_preview(
        edit_state: EditState,
        state: State<'_, AppState>,
    ) -> Result<Vec<u8>, String> {
        let lock = state.editor.lock().map_err(|e| e.to_string())?;
        let session = lock.as_ref().ok_or("No active editor session found")?;

        let preview = session.render_preview(&edit_state);
        let rgba_bytes = preview.into_raw();

        Ok(rgba_bytes)
    }

    #[derive(Debug, Serialize)]
    pub struct ExtractedStickerResponse {
        pub x: i32,
        pub y: i32,
        pub width: u32,
        pub height: u32,
        pub image: Vec<u8>,
        pub image_width: u32,
        pub image_height: u32,
    }

    #[tauri::command]
    pub fn extract_sticker(
        seed_u: f32,
        seed_v: f32,
        state: State<'_, AppState>,
    ) -> Result<ExtractedStickerResponse, String> {
        let lock = state.editor.lock().map_err(|e| e.to_string())?;
        let session = lock.as_ref().ok_or("No active editor session found")?;
        let extracted = session
            .extract_sticker(seed_u, seed_v)
            .ok_or("No distinct region found at that point")?;
        let (source_width, source_height) = session.proxy_source.dimensions();
        Ok(ExtractedStickerResponse {
            x: (extracted.bounds_norm.0 * source_width as f32).round() as i32,
            y: (extracted.bounds_norm.1 * source_height as f32).round() as i32,
            width: (extracted.bounds_norm.2 * source_width as f32)
                .round()
                .max(1.0) as u32,
            height: (extracted.bounds_norm.3 * source_height as f32)
                .round()
                .max(1.0) as u32,
            image_width: extracted.image.width(),
            image_height: extracted.image.height(),
            image: extracted.image.into_raw(),
        })
    }

    #[tauri::command]
    pub fn save_sticker(image: Vec<u8>, width: u32, height: u32) -> Result<String, String> {
        let expected_len = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or("Sticker dimensions are too large")? as usize;
        if image.len() != expected_len || width == 0 || height == 0 {
            return Err("Invalid sticker image data".to_string());
        }

        fs::create_dir_all("./stickers").map_err(|error| error.to_string())?;
        let path = format!("./stickers/sticker-{}.png", uuid::Uuid::new_v4());
        let buffer = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, image)
            .ok_or("Invalid sticker image data")?;
        buffer.save(&path).map_err(|error| error.to_string())?;
        Ok(path)
    }

    #[tauri::command]
    pub fn copy_sticker_to_clipboard(
        image: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let expected_len = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or("Sticker dimensions are too large")? as usize;
        if image.len() != expected_len || width == 0 || height == 0 {
            return Err("Invalid sticker image data".to_string());
        }

        let mut clipboard = Clipboard::new().map_err(|error| error.to_string())?;
        clipboard
            .set_image(ImageData {
                width: width as usize,
                height: height as usize,
                bytes: Cow::Owned(image),
            })
            .map_err(|error| error.to_string())
    }

    #[tauri::command]
    pub fn close_editor(state: State<'_, AppState>) -> Result<(), String> {
        let mut lock = state.editor.lock().map_err(|e| e.to_string())?;
        *lock = None;
        Ok(())
    }
}
