#[cfg(feature = "tauri-ipc")]
pub mod tauri_bindings {
    use crate::session::EditorSession;
    use crate::state::EditState;
    use crate::sticker_extractor::extract_sticker_from_seed;
    use serde::Serialize;
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
        let extracted = extract_sticker_from_seed(&session.proxy_source, seed_u, seed_v, 40, 70)
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
    pub fn close_editor(state: State<'_, AppState>) -> Result<(), String> {
        let mut lock = state.editor.lock().map_err(|e| e.to_string())?;
        *lock = None;
        Ok(())
    }
}
