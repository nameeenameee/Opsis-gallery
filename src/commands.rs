#[cfg(feature = "tauri-ipc")]
pub mod tauri_bindings {
    use crate::session::EditorSession;
    use crate::state::EditState;
    use std::sync::Mutex;
    use tauri::ipc::Response;
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
    ) -> Result<Response, String> {
        let lock = state.editor.lock().map_err(|e| e.to_string())?;
        let session = lock.as_ref().ok_or("No active editor session found")?;

        let preview = session.render_preview(&edit_state);
        let rgba_bytes = preview.into_raw();

        Ok(Response::new(rgba_bytes))
    }

    #[tauri::command]
    pub fn close_editor(state: State<'_, AppState>) -> Result<(), String> {
        let mut lock = state.editor.lock().map_err(|e| e.to_string())?;
        *lock = None;
        Ok(())
    }
}
