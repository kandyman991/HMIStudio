#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod persistence_host;
mod session_host;
use hmi_engineering_core::ProjectSnapshot;
use persistence_host::{recovery_project, remove_recovery, Host};
use serde::{Deserialize, Serialize};
use session_host::{WorkspaceReply, WorkspaceRequest};
use std::{path::PathBuf, sync::Mutex, time::Duration};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogResult};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ProjectAction {
    Open,
    NewProject,
    Save,
    SaveAs,
    Recover,
    ResolveUntitled,
    Exit,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectReply {
    snapshot: ProjectSnapshot,
    generation: u64,
    replaced: bool,
    message: String,
    pending_untitled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistenceStatus {
    pending_untitled: bool,
    autosave_message: String,
}

#[derive(Serialize)]
struct HostReply {
    #[serde(flatten)]
    reply: WorkspaceReply,
    generation: u64,
}

#[tauri::command]
fn engineering_request(
    request: WorkspaceRequest,
    generation: Option<u64>,
    state: tauri::State<'_, Mutex<Host>>,
) -> Result<HostReply, String> {
    let mut host = state
        .try_lock()
        .map_err(|_| "Project operation pending".to_string())?;
    if host.pending_untitled
        && !matches!(
            &request,
            WorkspaceRequest::Snapshot {} | WorkspaceRequest::Validate {}
        )
    {
        return Err("Resolve the untitled recovery before editing".into());
    }
    if !matches!(
        &request,
        WorkspaceRequest::Snapshot {} | WorkspaceRequest::Validate {}
    ) && generation != Some(host.generation)
    {
        return Err("Project session changed; refresh before editing".into());
    }
    if let WorkspaceRequest::Execute { transaction } = &request {
        use hmi_engineering_core::Command;
        if transaction.commands.iter().any(|cmd| {
            matches!(
                cmd,
                Command::InsertV2Component { .. }
                    | Command::CreateIndustrialSymbol { .. }
                    | Command::SetIndustrialSecurity { .. }
                    | Command::SetExpressionEnumerations { .. }
                    | Command::SetComponentActions { .. }
            ) || matches!(cmd, Command::UpsertStyle { style } if style.target.is_some_and(hmi_project_model::model::ComponentKind::requires_v2_contract))
        }) {
            return Err("V2 declaration editing is not supported by this workspace yet".into());
        }
    }
    let reply = session_host::request(&mut host.session, request);
    Ok(HostReply {
        reply,
        generation: host.generation,
    })
}

#[tauri::command]
fn persistence_status(state: tauri::State<'_, Mutex<Host>>) -> Result<PersistenceStatus, String> {
    let host = state
        .try_lock()
        .map_err(|_| "Project operation pending".to_string())?;
    Ok(PersistenceStatus {
        pending_untitled: host.pending_untitled,
        autosave_message: host.autosave_message.clone(),
    })
}

// All blocking native dialogs run off the UI thread, with one exclusive host
// operation in flight. No path or overwrite flag is accepted from JavaScript.
fn ask(app: &AppHandle, message: &str) -> MessageDialogResult {
    let mut dialog = app
        .dialog()
        .message(message)
        .title("HMI Studio")
        .buttons(MessageDialogButtons::YesNoCancel);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.parent(&window);
    }
    dialog.blocking_show_with_result()
}

fn pick(app: &AppHandle, recovery: bool, saving: bool) -> Result<Option<PathBuf>, String> {
    let mut dialog = app.dialog().file().add_filter("HMI Studio JSON", &["json"]);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    let selected = if saving {
        dialog
            .set_title("Save HMI project")
            .set_file_name("project.hmi.json")
            .blocking_save_file()
    } else {
        dialog
            .set_title(if recovery {
                "Select autosave.meta.json"
            } else {
                "Open HMI project"
            })
            .blocking_pick_file()
    };
    selected
        .map(|p| p.into_path().map_err(|e| e.to_string()))
        .transpose()
}

fn save(app: &AppHandle, host: &mut Host, save_as: bool) -> Result<bool, String> {
    if save_as || host.session.path().is_none() {
        let Some(path) = pick(app, false, true)? else {
            return Ok(false);
        };
        host.save_as(&path)?;
    } else {
        host.save()?;
    }
    Ok(true)
}

// Return None when canceled, Some(true) when discard was explicitly chosen.
// A failed/canceled save does not authorize a subsequent destructive action.
fn confirm_switch(app: &AppHandle, host: &mut Host) -> Result<Option<bool>, String> {
    if !host.session.is_dirty() {
        return Ok(Some(false));
    }
    match ask(app, "Save changes to the current project before continuing?\nYes: Save. No: Discard changes. Cancel: Keep editing.") {
        MessageDialogResult::Yes => Ok(save(app, host, false)?.then_some(false)),
        MessageDialogResult::No => Ok(Some(true)),
        _ => Ok(None),
    }
}

fn handle_project_action(app: &AppHandle, action: ProjectAction) -> Result<ProjectReply, String> {
    let state = app.state::<Mutex<Host>>();
    let mut host = state
        .try_lock()
        .map_err(|_| "Another project operation is pending".to_string())?;
    let mut replaced = false;
    let mut message = "Canceled".to_string();
    match action {
        ProjectAction::ResolveUntitled => {
            if host.pending_untitled {
                match ask(app, "An untitled recovery snapshot exists.\nYes: Recover. No: Discard recovery. Cancel: Leave it untouched.") {
                    MessageDialogResult::Yes => { host.recover_untitled()?; replaced = true; message = "Untitled project recovered".into(); }
                    MessageDialogResult::No => { host.discard_untitled()?; message = "Untitled recovery discarded".into(); }
                    _ => {}
                }
            } else {
                message = "No pending untitled recovery".into();
            }
        }
        ProjectAction::Exit => {
            if host.pending_untitled {
                // No edits are admitted while pending; preserve the snapshot.
                app.exit(0);
            } else if let Some(discard) = confirm_switch(app, &mut host)? {
                if discard || !host.session.is_dirty() {
                    remove_recovery(&host.active_recovery()?)?;
                }
                app.exit(0);
            }
        }
        _ if host.pending_untitled => return Err("Resolve untitled recovery first".into()),
        ProjectAction::Save | ProjectAction::SaveAs => {
            if save(app, &mut host, matches!(action, ProjectAction::SaveAs))? {
                message = host.autosave_message.clone();
            }
        }
        ProjectAction::NewProject => {
            if let Some(discard) = confirm_switch(app, &mut host)? {
                host.new_project(discard)?;
                replaced = true;
                message = "New untitled project".into();
            }
        }
        ProjectAction::Open | ProjectAction::Recover => {
            let recover = matches!(action, ProjectAction::Recover);
            if let Some(selected) = pick(app, recover, false)? {
                let path = if recover {
                    recovery_project(&selected)?
                } else {
                    selected
                };
                if let Some(discard_current) = confirm_switch(app, &mut host)? {
                    replaced = host.open_with(&path, discard_current, |has_recovery| {
                        if recover {
                            if !has_recovery { return Err("Recovery snapshot is missing".into()); }
                            return Ok(Some(true));
                        }
                        if !has_recovery { return Ok(Some(false)); }
                        match ask(app, "A recovery snapshot exists for this project.\nYes: Recover. No: Open saved project and discard recovery. Cancel: Keep current project.") {
                            MessageDialogResult::Yes => Ok(Some(true)),
                            MessageDialogResult::No => Ok(Some(false)),
                            _ => Ok(None),
                        }
                    })?;
                    if replaced {
                        message = "Project opened".into();
                    }
                }
            }
        }
    }
    Ok(ProjectReply {
        snapshot: host.session.snapshot(),
        generation: host.generation,
        replaced,
        message,
        pending_untitled: host.pending_untitled,
    })
}

#[tauri::command]
async fn project_action(app: AppHandle, action: ProjectAction) -> Result<ProjectReply, String> {
    tauri::async_runtime::spawn_blocking(move || handle_project_action(&app, action))
        .await
        .map_err(|e| e.to_string())?
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let recovery = app.path().app_data_dir()?.join("project-recovery");
            let host = Host::new(&recovery).map_err(std::io::Error::other)?;
            app.manage(Mutex::new(host));
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_secs(30));
                if let Ok(mut host) = handle.state::<Mutex<Host>>().try_lock() {
                    host.autosave();
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engineering_request,
            project_action,
            persistence_status
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let handle = window.app_handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(error) = handle_project_action(&handle, ProjectAction::Exit) {
                        handle
                            .dialog()
                            .message(error)
                            .title("HMI Studio — close canceled")
                            .blocking_show();
                    }
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("HMI Studio startup failed");
}

#[cfg(test)]
mod persistence_ipc_tests {
    use super::*;
    #[test]
    fn native_action_contract_contains_no_webview_paths_or_flags() {
        assert!(matches!(
            serde_json::from_str::<ProjectAction>(r#""open""#).unwrap(),
            ProjectAction::Open
        ));
        assert!(matches!(
            serde_json::from_str::<ProjectAction>(r#""saveAs""#).unwrap(),
            ProjectAction::SaveAs
        ));
        for input in [
            r#"{"action":"open","path":"/tmp/secret.hmi.json"}"#,
            r#"{"action":"saveAs","overwrite":true}"#,
            r#""saveForce""#,
        ] {
            assert!(serde_json::from_str::<ProjectAction>(input).is_err());
        }
        let caps: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        assert_eq!(caps["permissions"], serde_json::json!([]));
    }
}
