#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod session_host;
use hmi_engineering_core::ProjectSession;
use session_host::{WorkspaceReply, WorkspaceRequest};
use std::sync::Mutex;
use tauri::Manager;

#[tauri::command]
fn engineering_request(
    request: WorkspaceRequest,
    state: tauri::State<'_, Mutex<ProjectSession>>,
) -> Result<WorkspaceReply, String> {
    let mut session = state
        .lock()
        .map_err(|_| "Engineering core unavailable".to_string())?;
    Ok(session_host::request(&mut session, request))
}

// The only explicit discard operation in this in-memory slice. The UI must
// confirm first; there is no access to project paths or the filesystem.
#[tauri::command]
fn discard_and_exit(app: tauri::AppHandle) {
    app.exit(0);
}

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(session_host::initial_session()))
        .invoke_handler(tauri::generate_handler![
            engineering_request,
            discard_and_exit
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<Mutex<ProjectSession>>();
                if state.lock().map(|s| s.is_dirty()).unwrap_or(true) {
                    api.prevent_close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("HMI Studio startup failed");
}
