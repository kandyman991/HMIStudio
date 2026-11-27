#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod persistence_host;
mod runtime_host;
mod session_host;
use hmi_engineering_core::ProjectSnapshot;
use persistence_host::{recovery_project, Host};
use runtime_host::{RuntimeOutcome, RuntimeRequest};
use serde::{Deserialize, Serialize};
use session_host::{WorkspaceReply, WorkspaceRequest};
use std::{path::PathBuf, sync::Mutex, time::Duration};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogResult};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ProjectAction {
    Open,
    ImportLegacy,
    ImportLegacyRevision,
    ImportLegacyPackage,
    OpenReview,
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
    persistence: PersistenceStatus,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistenceStatus {
    generation: u64,
    pending_untitled: bool,
    autosave_message: String,
    review_required: bool,
}

impl From<&Host> for PersistenceStatus {
    fn from(host: &Host) -> Self {
        Self {
            generation: host.generation,
            pending_untitled: host.pending_untitled,
            autosave_message: host.persistence_message(),
            review_required: host.is_review(),
        }
    }
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
    let reply = host.request(request, generation)?;
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
    Ok(PersistenceStatus::from(&*host))
}

#[tauri::command]
async fn simulation_request(
    app: AppHandle,
    request: RuntimeRequest,
    generation: u64,
) -> Result<RuntimeOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Mutex<Host>>();
        let mut host = match state.try_lock() {
            Ok(host) => host,
            Err(std::sync::TryLockError::WouldBlock) => return Ok(RuntimeOutcome::Busy),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err("Simulation host unavailable".into())
            }
        };
        Ok(host.simulation_outcome(generation, request, || {
            ask(&app, "Confirm the selected action in isolated simulation?")
                == MessageDialogResult::Yes
        }))
    })
    .await
    .map_err(|error| error.to_string())?
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

fn pick(app: &AppHandle, title: &str, saving: bool) -> Result<Option<PathBuf>, String> {
    let mut dialog = app
        .dialog()
        .file()
        .add_filter("JSON files", &["json"])
        .set_title(title);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    let selected = if saving {
        dialog
            .set_file_name("project.hmi.json")
            .blocking_save_file()
    } else {
        dialog.blocking_pick_file()
    };
    selected
        .map(|p| p.into_path().map_err(|e| e.to_string()))
        .transpose()
}

fn pick_runtime_package(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    let mut dialog = app
        .dialog()
        .file()
        .add_filter("Legacy runtime package", &["hmipkg"])
        .set_title("Select legacy runtime package");
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    dialog
        .blocking_pick_file()
        .map(|p| p.into_path().map_err(|e| e.to_string()))
        .transpose()
}

fn pick_revision_directory(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Select legacy .hmistudio revision directory");
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    dialog
        .blocking_pick_folder()
        .map(|p| p.into_path().map_err(|e| e.to_string()))
        .transpose()
}

fn pick_review(app: &AppHandle, saving: bool) -> Result<Option<PathBuf>, String> {
    let mut dialog = app
        .dialog()
        .file()
        .add_filter("HMI Studio review", &["hmi-review"]);
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    let selected = if saving {
        dialog
            .set_title("Save new review bundle")
            .set_file_name("project.hmi-review")
            .blocking_save_file()
    } else {
        dialog.set_title("Open review bundle").blocking_pick_file()
    };
    selected
        .map(|p| p.into_path().map_err(|e| e.to_string()))
        .transpose()
}

fn save(app: &AppHandle, host: &mut Host, save_as: bool) -> Result<bool, String> {
    if host.is_review() {
        let Some(path) = pick_review(app, true)? else {
            return Ok(false);
        };
        host.save_as(&path)?;
        return Ok(true);
    }
    if save_as || host.path().is_none() {
        let Some(path) = pick(app, "Save HMI project", true)? else {
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
    if !host.is_dirty() {
        return Ok(Some(false));
    }
    match ask(app, "Save changes to the current project before continuing?\nYes: Save. No: Discard changes. Cancel: Keep editing.") {
        MessageDialogResult::Yes => Ok(save(app, host, false)?.then_some(false)),
        MessageDialogResult::No => Ok(Some(true)),
        _ => Ok(None),
    }
}

fn handle_project_action(
    app: &AppHandle,
    action: ProjectAction,
    generation: Option<u64>,
) -> Result<ProjectReply, String> {
    let state = app.state::<Mutex<Host>>();
    let mut host = state
        .try_lock()
        .map_err(|_| "Another project operation is pending".to_string())?;
    if let Some(generation) = generation {
        host.check_generation(generation)?;
    }
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
                host.stop_simulation();
                app.exit(0);
            } else if let Some(discard) = confirm_switch(app, &mut host)? {
                if discard || !host.is_dirty() {
                    host.clear_active_recovery()?;
                }
                host.stop_simulation();
                app.exit(0);
            }
        }
        _ if host.pending_untitled => return Err("Resolve untitled recovery first".into()),
        ProjectAction::Save | ProjectAction::SaveAs => {
            if save(app, &mut host, matches!(action, ProjectAction::SaveAs))? {
                message = host.persistence_message();
            }
        }
        ProjectAction::NewProject => {
            if let Some(discard) = confirm_switch(app, &mut host)? {
                host.new_project(discard)?;
                replaced = true;
                message = "New untitled project".into();
            }
        }
        ProjectAction::ImportLegacy => {
            if let Some(source) = pick(app, "Select portable legacy JSON", false)? {
                if ask(app, "Import this portable legacy project? The new local review bundle retains the exact source JSON, including private project data. No production authorization is granted. Choose a private destination.") == MessageDialogResult::Yes {
                    if let Some(target) = pick_review(app, true)? {
                        host.check_portable_import_paths(&source, &target)?;
                        if let Some(discard) = confirm_switch(app, &mut host)? {
                            host.import_review(&source, &target, discard)?;
                            replaced = true;
                            message = host.persistence_message();
                        }
                    }
                }
            }
        }
        ProjectAction::ImportLegacyRevision => {
            if let Some(source) = pick_revision_directory(app)? {
                if ask(app, "Import the currently published legacy revision? The new local review bundle retains the exact descriptor, HEAD and selected revision, including private history metadata. It does not execute history or grant production authorization. Choose a private destination outside the legacy store.") == MessageDialogResult::Yes {
                    if let Some(target) = pick_review(app, true)? {
                        host.check_revision_import_paths(&source, &target)?;
                        if let Some(discard) = confirm_switch(app, &mut host)? {
                            host.import_revision_review(&source, &target, discard)?;
                            replaced = true;
                            message = host.persistence_message();
                        }
                    }
                }
            }
        }
        ProjectAction::ImportLegacyPackage => {
            if let Some(source) = pick_runtime_package(app)? {
                if ask(app, "Import this legacy runtime package? The new local review bundle retains the exact archive, including private project, runtime and signature metadata. No publisher trust, live values or production authorization are granted. Choose a private destination.") == MessageDialogResult::Yes {
                    if let Some(target) = pick_review(app, true)? {
                        host.check_package_import_paths(&source, &target)?;
                        if let Some(discard) = confirm_switch(app, &mut host)? {
                            host.import_package_review(&source, &target, discard)?;
                            replaced = true;
                            message = host.persistence_message();
                        }
                    }
                }
            }
        }
        ProjectAction::OpenReview => {
            if let Some(path) = pick_review(app, false)? {
                if let Some(discard) = confirm_switch(app, &mut host)? {
                    replaced = host.open_review_with(&path, discard, |pending| {
                        if !pending { return Ok(Some(false)); }
                        match ask(app, "A recovery snapshot exists for this review bundle.\nYes: Recover edits. No: Open saved bundle and discard recovery. Cancel: Keep current project.") {
                            MessageDialogResult::Yes => Ok(Some(true)),
                            MessageDialogResult::No => Ok(Some(false)),
                            _ => Ok(None),
                        }
                    })?;
                    if replaced {
                        message = host.persistence_message();
                    }
                }
            }
        }
        ProjectAction::Open | ProjectAction::Recover => {
            let recover = matches!(action, ProjectAction::Recover);
            let title = if recover {
                "Select autosave.meta.json"
            } else {
                "Open HMI project"
            };
            if let Some(selected) = pick(app, title, false)? {
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
        snapshot: host.snapshot(),
        generation: host.generation,
        replaced,
        message,
        persistence: PersistenceStatus::from(&*host),
    })
}

#[tauri::command]
async fn project_action(
    app: AppHandle,
    action: ProjectAction,
    generation: u64,
) -> Result<ProjectReply, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let importing = matches!(
            &action,
            ProjectAction::ImportLegacy
                | ProjectAction::ImportLegacyRevision
                | ProjectAction::ImportLegacyPackage
        );
        let result = handle_project_action(&app, action, Some(generation));
        if importing {
            if let Err(error) = &result {
                app.dialog()
                    .message(error.as_str())
                    .title("HMI Studio — legacy import refused")
                    .blocking_show();
            }
        }
        result
    })
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
            persistence_status,
            simulation_request
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let handle = window.app_handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(error) = handle_project_action(&handle, ProjectAction::Exit, None) {
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
        assert!(matches!(
            serde_json::from_str::<ProjectAction>(r#""importLegacyPackage""#).unwrap(),
            ProjectAction::ImportLegacyPackage
        ));
        for input in [
            r#"{"action":"open","path":"/tmp/secret.hmi.json"}"#,
            r#"{"action":"saveAs","overwrite":true}"#,
            r#""saveForce""#,
            r#"{"action":"importLegacy","source":"/tmp/source.json"}"#,
            r#"{"action":"importLegacyRevision","source":"/tmp/store.hmistudio"}"#,
            r#"{"action":"importLegacyPackage","source":"/tmp/source.hmipkg","consent":true}"#,
            r#"{"action":"openReview","path":"/tmp/hidden.hmi-review"}"#,
            r#""publish""#,
            r#""connectLive""#,
        ] {
            assert!(serde_json::from_str::<ProjectAction>(input).is_err());
        }
        let caps: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        assert_eq!(caps["permissions"], serde_json::json!([]));
    }
}
