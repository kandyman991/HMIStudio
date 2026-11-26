//! Native persistence boundary. The webview never supplies a filesystem path.
use hmi_engineering_core::{
    fs::{FileSystem, StdFileSystem},
    legacy_quarantine::{
        requires_review, ProductionCandidate, QuarantinedImport, MAX_LEGACY_SOURCE_BYTES,
    },
    persistence::sidecar_dir,
    AutosaveOutcome, ProjectSession, ProjectSnapshot, SaveOptions, SessionConfig,
};
use hmi_project_model::{load_project, MigrationRegistry};
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

type Result<T> = std::result::Result<T, String>;
const RECOVERY: &str = "autosave.meta.json";

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(m) => {
            if m.file_type().is_symlink() {
                return Err(format!(
                    "Symbolic links are not allowed: {}",
                    path.display()
                ));
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if m.file_attributes() & 0x400 != 0 {
                    return Err(format!(
                        "Reparse points are not allowed: {}",
                        path.display()
                    ));
                }
            }
            Ok(Some(m))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(err(e)),
    }
}

fn safe_ancestors(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err("An absolute native path is required".into());
    }
    for p in path.ancestors().skip(1) {
        if let Some(m) = metadata(p)? {
            if !m.is_dir() {
                return Err(format!("Not a directory: {}", p.display()));
            }
        }
    }
    Ok(())
}

fn safe_file(path: &Path) -> Result<bool> {
    safe_ancestors(path)?;
    match metadata(path)? {
        Some(m) if m.is_file() => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if m.nlink() != 1 {
                    return Err(format!("Hard links are not supported: {}", path.display()));
                }
            }
            Ok(true)
        }
        Some(_) => Err(format!("Not a regular file: {}", path.display())),
        None => Ok(false),
    }
}

fn safe_dir(path: &Path) -> Result<()> {
    safe_ancestors(path)?;
    if let Some(m) = metadata(path)? {
        if !m.is_dir() {
            return Err(format!("Not a directory: {}", path.display()));
        }
    }
    Ok(())
}

// Only the expected sidecar layout is allowed. Do not follow arbitrary links
// or traverse unrelated subdirectories.
fn safe_sidecar(path: &Path) -> Result<()> {
    safe_dir(path)?;
    if metadata(path)?.is_none() {
        return Ok(());
    }
    for entry in fs::read_dir(path).map_err(err)? {
        let child = entry.map_err(err)?.path();
        if child.file_name().is_some_and(|n| n == "backups") {
            safe_dir(&child)?;
            for backup in fs::read_dir(&child).map_err(err)? {
                safe_file(&backup.map_err(err)?.path())?;
            }
        } else {
            safe_file(&child)?;
        }
    }
    Ok(())
}

fn normalize_review(path: &Path) -> Result<PathBuf> {
    safe_file(path)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid review bundle filename")?;
    if name.contains(':') || !name.to_ascii_lowercase().ends_with(".hmi-review") {
        return Err("Select an .hmi-review bundle".into());
    }
    let parent = path.parent().ok_or("Missing bundle directory")?;
    let target = parent.canonicalize().map_err(err)?.join(name);
    safe_file(&target)?;
    Ok(target)
}

fn normalize_project(path: &Path) -> Result<PathBuf> {
    safe_file(path)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid project filename")?;
    if name.contains(':') || !name.to_ascii_lowercase().ends_with(".hmi.json") {
        return Err("Select an .hmi.json project".into());
    }
    let parent = path.parent().ok_or("Missing project directory")?;
    let parent = parent.canonicalize().map_err(err)?;
    let target = parent.join(name);
    safe_file(&target)?;
    safe_sidecar(&sidecar_dir(&target))?;
    Ok(target)
}

// Advisory OS lock survives while the handle is alive and is released on
// process termination. Never unlink the persistent lock filename.
fn lock_dir(dir: &Path) -> Result<File> {
    safe_dir(dir)?;
    fs::create_dir_all(dir).map_err(err)?;
    safe_sidecar(dir)?;
    let path = dir.join("writer.lock");
    safe_file(&path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(err)?;
    file.try_lock()
        .map_err(|e| format!("Storage already in use or locking unavailable: {e}"))?;
    Ok(file)
}

// Revalidate each engine access as well as the operation preflight. This is
// defense in depth for trusted local storage, not a race-proof sandbox against
// another hostile process that can rename directories between check and use.
struct GuardedFileSystem;
impl FileSystem for GuardedFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        safe_file(path).map_err(io::Error::other)?;
        StdFileSystem.read(path)
    }
    fn read_limited(&self, path: &Path, limit: usize) -> io::Result<Vec<u8>> {
        safe_file(path).map_err(io::Error::other)?;
        StdFileSystem.read_limited(path, limit)
    }
    fn write_synced(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        safe_file(path).map_err(io::Error::other)?;
        StdFileSystem.write_synced(path, data)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        safe_file(from).map_err(io::Error::other)?;
        safe_file(to).map_err(io::Error::other)?;
        StdFileSystem.rename(from, to)
    }
    fn link_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        safe_file(from).map_err(io::Error::other)?;
        if safe_file(to).map_err(io::Error::other)? {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "bundle exists"));
        }
        StdFileSystem.link_new(from, to)
    }
    fn unlink_staged(&self, path: &Path) -> io::Result<()> {
        // Exclusive publication temporarily gives the staged inode two links.
        safe_ancestors(path).map_err(io::Error::other)?;
        if !metadata(path)
            .map_err(io::Error::other)?
            .is_some_and(|m| m.is_file())
        {
            return Err(io::Error::other("invalid staged file"));
        }
        StdFileSystem.remove_file(path)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        safe_file(path).map_err(io::Error::other)?;
        StdFileSystem.remove_file(path)
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        safe_dir(path).map_err(io::Error::other)?;
        StdFileSystem.create_dir_all(path)
    }
    fn exists(&self, path: &Path) -> bool {
        safe_file(path).unwrap_or(false)
    }
    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        safe_dir(path).map_err(io::Error::other)?;
        for entry in fs::read_dir(path)? {
            safe_file(&entry?.path()).map_err(io::Error::other)?;
        }
        StdFileSystem.list_dir(path)
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        safe_dir(path).map_err(io::Error::other)?;
        StdFileSystem.sync_dir(path)
    }
}

fn config(dir: &Path) -> SessionConfig {
    SessionConfig {
        untitled_recovery_dir: Some(dir.to_path_buf()),
        ..SessionConfig::default()
    }
}

pub struct Host {
    session: ProjectSession,
    review: Option<QuarantinedImport>,
    pub generation: u64,
    untitled: PathBuf,
    _instance_lock: File,
    project_lock: Option<File>,
    pub pending_untitled: bool,
    pub autosave_message: String,
}

impl Host {
    pub fn new(dir: &Path) -> Result<Self> {
        safe_dir(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)
                .map_err(err)?;
        }
        let instance_lock = lock_dir(dir)?;
        let pending = safe_file(&dir.join(RECOVERY))?;
        let project = load_project(include_str!(
            "../../crates/project-model/fixtures/valid/minimal.hmi.json"
        ))
        .map_err(err)?
        .project;
        Ok(Self {
            session: ProjectSession::with_fs(project, config(dir), Arc::new(GuardedFileSystem)),
            review: None,
            generation: 0,
            untitled: dir.to_path_buf(),
            _instance_lock: instance_lock,
            project_lock: None,
            pending_untitled: pending,
            autosave_message: "Autosave ready".into(),
        })
    }

    pub fn is_review(&self) -> bool {
        self.review.is_some()
    }

    pub fn is_dirty(&self) -> bool {
        self.review.as_ref().map_or_else(
            || self.session.is_dirty(),
            QuarantinedImport::is_dirty,
        )
    }

    pub fn path(&self) -> Option<&Path> {
        self.review
            .as_ref()
            .map(QuarantinedImport::path)
            .or_else(|| self.session.path())
    }

    pub fn snapshot(&self) -> ProjectSnapshot {
        self.review
            .as_ref()
            .map_or_else(|| self.session.snapshot(), QuarantinedImport::snapshot)
    }

    pub fn request(
        &mut self,
        request: crate::session_host::WorkspaceRequest,
    ) -> crate::session_host::WorkspaceReply {
        match &mut self.review {
            Some(review) => crate::session_host::review_request(review, request),
            None => crate::session_host::request(&mut self.session, request),
        }
    }

    /// Import-specific screening before a future production or live consumer
    /// may obtain the borrowed project. No actual consumer exists yet.
    pub fn production_candidate(&self) -> Result<ProductionCandidate<'_>> {
        if self.review.is_some() || self.pending_untitled {
            return Err("Production/live admission blocked: review or recovery required".into());
        }
        ProductionCandidate::from_session(&self.session).map_err(err)
    }

    fn preflight(&self) -> Result<()> {
        safe_sidecar(&self.untitled)?;
        if let Some(path) = self.path() {
            safe_file(path)?;
            if !self.is_review() {
                safe_sidecar(&sidecar_dir(path))?;
            }
        }
        Ok(())
    }

    pub fn recover_untitled(&mut self) -> Result<()> {
        self.preflight()?;
        if !self.pending_untitled {
            return Err("No pending untitled recovery".into());
        }
        let opened = ProjectSession::recover_untitled(
            config(&self.untitled),
            Arc::new(GuardedFileSystem),
            &MigrationRegistry::builtin(),
        )
        .map_err(err)?;
        if requires_review(opened.session.project()) {
            return Err("Imported projects must be opened as verified review bundles".into());
        }
        if opened.session.project().has_v2_features() && opened.report.has_errors() {
            return Err("Invalid v2 project; original data preserved".into());
        }
        self.review = None;
        self.session = opened.session;
        self.generation += 1;
        self.pending_untitled = false;
        Ok(())
    }

    pub fn discard_untitled(&mut self) -> Result<()> {
        self.preflight()?;
        if self.pending_untitled {
            fs::remove_file(self.untitled.join(RECOVERY)).map_err(err)?;
            self.pending_untitled = false;
        }
        Ok(())
    }

    // Prepare and lock the candidate before replacing the current session.
    // A failed open/recovery leaves the old project and its lock untouched.
    pub fn open_with(
        &mut self,
        path: &Path,
        discard_current: bool,
        decide: impl FnOnce(bool) -> Result<Option<bool>>,
    ) -> Result<bool> {
        self.preflight()?;
        if self.pending_untitled {
            return Err("Resolve untitled recovery first".into());
        }
        let path = normalize_project(path)?;
        if self.path() == Some(path.as_path()) {
            return Err("This project is already open".into());
        }
        let dir = sidecar_dir(&path);
        let guard = lock_dir(&dir)?;
        let has_recovery = safe_file(&dir.join(RECOVERY))?;
        let Some(recover) = decide(has_recovery)? else {
            return Ok(false);
        };
        if self.is_dirty() && !discard_current {
            return Err("Unsaved changes".into());
        }
        let opened = if recover {
            ProjectSession::recover(
                &path,
                config(&self.untitled),
                Arc::new(GuardedFileSystem),
                &MigrationRegistry::builtin(),
            )
        } else {
            ProjectSession::open(
                &path,
                config(&self.untitled),
                Arc::new(GuardedFileSystem),
                &MigrationRegistry::builtin(),
            )
        }
        .map_err(err)?;
        if requires_review(opened.session.project()) {
            return Err("Imported projects must be opened as verified review bundles".into());
        }
        if opened.session.project().has_v2_features() && opened.report.has_errors() {
            return Err("Invalid v2 project; original data preserved".into());
        }
        if has_recovery && !recover {
            fs::remove_file(dir.join(RECOVERY)).map_err(err)?;
        }
        // The old writer lock is still held. Remove its recovery only after
        // the new project has been fully parsed and explicitly accepted.
        self.clear_active_recovery()?;
        self.review = None;
        self.session = opened.session;
        self.generation += 1;
        self.project_lock = Some(guard);
        self.autosave_message = "Autosave ready".into();
        Ok(true)
    }

    pub fn new_project(&mut self, discard: bool) -> Result<()> {
        self.preflight()?;
        if self.pending_untitled || (self.is_dirty() && !discard) {
            return Err("Resolve unsaved changes or recovery first".into());
        }
        let project = load_project(include_str!(
            "../../crates/project-model/fixtures/valid/minimal.hmi.json"
        ))
        .map_err(err)?
        .project;
        self.clear_active_recovery()?;
        self.review = None;
        self.session =
            ProjectSession::with_fs(project, config(&self.untitled), Arc::new(GuardedFileSystem));
        self.generation += 1;
        self.project_lock = None;
        self.autosave_message = "Autosave ready".into();
        Ok(())
    }

    fn prepare_review_switch(&self, discard: bool) -> Result<()> {
        self.preflight()?;
        if self.pending_untitled || (self.is_dirty() && !discard) {
            return Err("Resolve unsaved changes or recovery first".into());
        }
        Ok(())
    }

    fn activate_review(&mut self, candidate: QuarantinedImport) {
        // The bundle has already been published/verified; never report it
        // rolled back if cleaning the previous recovery slot fails.
        let cleanup = self.clear_active_recovery();
        self.autosave_message = review_message(&candidate);
        if cleanup.is_err() {
            self.autosave_message.push_str("; previous recovery retained");
        }
        self.review = Some(candidate);
        self.project_lock = None;
        self.generation += 1;
    }

    pub fn import_review(&mut self, source: &Path, target: &Path, discard: bool) -> Result<()> {
        self.prepare_review_switch(discard)?;
        if !safe_file(source)? {
            return Err("Legacy source does not exist".into());
        }
        let target = normalize_review(target)?;
        if safe_file(&target)? {
            return Err("Choose a new review bundle filename".into());
        }
        let fs = Arc::new(GuardedFileSystem);
        let bytes = fs.read_limited(source, MAX_LEGACY_SOURCE_BYTES).map_err(err)?;
        let candidate =
            QuarantinedImport::import_new(&bytes, &target, config(&self.untitled), fs)
                .map_err(err)?;
        self.activate_review(candidate);
        Ok(())
    }

    pub fn open_review(&mut self, path: &Path, discard: bool) -> Result<()> {
        self.prepare_review_switch(discard)?;
        let path = normalize_review(path)?;
        if self.path() == Some(path.as_path()) {
            return Err("This review bundle is already open".into());
        }
        let candidate =
            QuarantinedImport::reopen(&path, config(&self.untitled), Arc::new(GuardedFileSystem))
                .map_err(err)?;
        self.activate_review(candidate);
        Ok(())
    }

    pub fn save(&mut self) -> Result<()> {
        self.preflight()?;
        if self.pending_untitled {
            return Err("Resolve untitled recovery first".into());
        }
        if self.is_review() {
            return Err("Review bundles are immutable; save a new review copy".into());
        }
        let outcome = self.session.save(SaveOptions::default()).map_err(err)?;
        self.autosave_message = save_message(&outcome.warnings);
        Ok(())
    }

    pub fn save_as(&mut self, path: &Path) -> Result<()> {
        self.preflight()?;
        if self.pending_untitled {
            return Err("Resolve untitled recovery first".into());
        }
        if let Some(review) = &mut self.review {
            let target = normalize_review(path)?;
            if safe_file(&target)? {
                return Err("Choose a new review bundle filename".into());
            }
            review.save_copy(&target).map_err(err)?;
            self.autosave_message = review_message(review);
            return Ok(());
        }
        let path = normalize_project(path)?;
        if self.session.path() == Some(path.as_path()) {
            return self.save();
        }
        // Do not overwrite an unrelated project or its recovery. A native
        // save dialog selecting an existing file is not permission to replace.
        if safe_file(&path)? {
            return Err("Choose a new filename; replacing an unrelated project is disabled".into());
        }
        let dir = sidecar_dir(&path);
        let guard = lock_dir(&dir)?;
        if safe_file(&dir.join(RECOVERY))? {
            return Err("Target has recovery data; choose a different filename".into());
        }
        let outcome = self
            .session
            .save_as(&path, SaveOptions::default())
            .map_err(err)?;
        self.project_lock = Some(guard);
        self.autosave_message = save_message(&outcome.warnings);
        Ok(())
    }

    pub fn autosave(&mut self) {
        if self.pending_untitled {
            return;
        }
        if let Some(review) = &self.review {
            self.autosave_message = review_message(review);
            return;
        }
        self.autosave_message = match self
            .preflight()
            .and_then(|_| self.session.autosave().map_err(err))
        {
            Ok(AutosaveOutcome::Clean) => "No unsaved edits to recover".into(),
            Ok(_) => "Recovery snapshot current".into(),
            Err(e) => format!("Autosave failed: {e}"),
        };
    }

    // Discard only the active project's recovery after explicit confirmation.
    // Call only after a successful candidate open or immediately before exit.
    pub fn clear_active_recovery(&self) -> Result<()> {
        if self.is_review() {
            return Ok(());
        }
        remove_recovery(&self.active_recovery()?)
    }

    pub fn active_recovery(&self) -> Result<PathBuf> {
        self.preflight()?;
        Ok(self
            .session
            .path()
            .map(sidecar_dir)
            .unwrap_or_else(|| self.untitled.clone())
            .join(RECOVERY))
    }
}

fn review_message(review: &QuarantinedImport) -> String {
    let mut message = if review.is_dirty() {
        "Review required — unsaved edits; save a new review copy (no autosave)".to_string()
    } else {
        "Review required — production/live blocked; review bundle saved".to_string()
    };
    if !review.warnings().is_empty() {
        message.push_str("; ");
        message.push_str(&review.warnings().join("; "));
    }
    message
}

fn save_message(warnings: &[String]) -> String {
    if warnings.is_empty() {
        "Saved".into()
    } else {
        format!("Saved with warnings: {}", warnings.join("; "))
    }
}

pub fn remove_recovery(path: &Path) -> Result<()> {
    if safe_file(path)? {
        fs::remove_file(path).map_err(err)?;
    }
    Ok(())
}

pub fn recovery_project(envelope: &Path) -> Result<PathBuf> {
    if !safe_file(envelope)? || envelope.file_name().is_none_or(|n| n != RECOVERY) {
        return Err("Select a project autosave.meta.json envelope".into());
    }
    let dir = envelope.parent().ok_or("Missing sidecar directory")?;
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(".studio"))
        .ok_or("Invalid project sidecar directory")?;
    normalize_project(&dir.parent().ok_or("Missing project directory")?.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmi_engineering_core::{Command, Transaction};
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static N: AtomicUsize = AtomicUsize::new(0);
            let p = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "studio-host-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn edit(host: &mut Host, name: &str) {
        let screen = host.session.project().screens[0].id.clone();
        host.session
            .execute(Transaction::single(Command::RenameScreen {
                screen,
                name: name.into(),
            }))
            .unwrap();
    }

    #[test]
    fn native_review_lifecycle_blocks_plain_persistence_and_production() {
        use crate::session_host::WorkspaceRequest;

        let t = Temp::new();
        let source = t.0.join("legacy.json");
        let target = t.0.join("legacy.hmi-review");
        let copy = t.0.join("edited.hmi-review");
        let original = br#"{"schemaVersion":1,"metadata":{"name":"Synthetic","defaultScreenId":"screen/a","targetProfiles":[]},"screens":{"screen/a":{"id":"screen/a","title":"Main","widthDp":640,"heightDp":480,"components":{"label-a":{"id":"label-a","type":"label","properties":{"text":"Synthetic"}}}}},"variables":{},"externalSources":{},"externalMappings":{}}"#;
        fs::write(&source, original).unwrap();
        let mut host = Host::new(&t.0.join("app")).unwrap();
        assert!(host.production_candidate().is_ok());
        host.import_review(&source, &target, false).unwrap();
        assert!(host.is_review());
        assert!(host.production_candidate().is_err());
        assert_eq!(fs::read(&source).unwrap(), original);
        let before = host.snapshot();
        let _ = host.request(WorkspaceRequest::Execute {
            transaction: Transaction::single(Command::SetExtension {
                key: "hmistudio.importQuarantine".into(),
                value: None,
            })
            .at_revision(before.revision),
        });
        assert_eq!(host.snapshot(), before);
        let screen = before.project.screens[0].id.clone();
        let _ = host.request(WorkspaceRequest::Execute {
            transaction: Transaction::single(Command::RenameScreen {
                screen,
                name: "Reviewed edit".into(),
            })
            .at_revision(before.revision),
        });
        assert!(host.is_dirty());
        assert!(host.new_project(false).is_err());
        assert!(host.save().is_err());
        assert!(host.save_as(&source).is_err());
        assert!(host.save_as(&t.0.join("plain.hmi.json")).is_err());
        assert!(host.production_candidate().is_err());
        host.save_as(&copy).unwrap();
        assert!(!host.is_dirty());
        host.new_project(false).unwrap();
        assert!(host.production_candidate().is_ok());
        host.open_review(&copy, false).unwrap();
        assert_eq!(host.snapshot().project.screens[0].name, "Reviewed edit");
        assert!(host.production_candidate().is_err());
        let stripped = t.0.join("stripped.hmi.json");
        let mut project = host.snapshot().project;
        project.extensions.clear();
        fs::write(&stripped, hmi_project_model::save_project(&project).unwrap()).unwrap();
        assert!(host.open_with(&stripped, false, |_| Ok(Some(false))).is_err());
        assert!(host.is_review());
        assert_eq!(fs::read(&source).unwrap(), original);
    }

    #[test]
    fn v2_project_opens_edits_saves_and_recovers_without_losing_declarations() {
        let t = Temp::new();
        let path = t.0.join("v2.hmi.json");
        let mut host = Host::new(&t.0.join("app")).unwrap();
        let mut project = host.session.project().clone();
        project.v2.industrial.security.auto_logout_seconds = Some(30);
        project
            .extensions
            .insert("synthetic.exact".into(), serde_json::json!(u64::MAX));
        let original = hmi_project_model::save_project(&project).unwrap();
        fs::write(&path, &original).unwrap();
        assert!(host.open_with(&path, false, |_| Ok(Some(false))).unwrap());
        let command: Command = serde_json::from_value(serde_json::json!({
            "op":"insertV2Component","parent":"n-root","id":"n-line",
            "contract":{"kind":"line","start":{"x":0,"y":0},"end":{"x":12,"y":5},"strokeColor":"#000000","strokeWidth":1}
        })).unwrap();
        host.session.execute(Transaction::single(command)).unwrap();
        host.save().unwrap();
        let saved = host.session.project().clone();
        assert_eq!(saved.v2.industrial, project.v2.industrial);
        assert_eq!(saved.extensions, project.extensions);
        edit(&mut host, "Recovered v2");
        host.autosave();
        drop(host);
        let mut reopened = Host::new(&t.0.join("app-b")).unwrap();
        assert!(reopened
            .open_with(&path, false, |_| Ok(Some(true)))
            .unwrap());
        assert_eq!(reopened.session.project().v2, saved.v2);
        assert_eq!(reopened.session.project().extensions, saved.extensions);
        assert_eq!(reopened.session.project().screens[0].name, "Recovered v2");
        reopened.save().unwrap();
        assert_eq!(
            hmi_project_model::load_project(&fs::read_to_string(&path).unwrap())
                .unwrap()
                .project,
            *reopened.session.project()
        );
    }

    #[test]
    fn save_open_lock_conflict_and_recovery() {
        let t = Temp::new();
        let path = t.0.join("line.hmi.json");
        let mut a = Host::new(&t.0.join("app-a")).unwrap();
        assert!(Host::new(&t.0.join("app-a")).is_err());
        edit(&mut a, "Saved");
        a.save_as(&path).unwrap();
        assert!(!a.session.is_dirty());
        edit(&mut a, "Recover me");
        a.autosave();
        let mut b = Host::new(&t.0.join("app-b")).unwrap();
        assert!(b.open_with(&path, false, |_| Ok(Some(true))).is_err());
        drop(a);
        assert!(b
            .open_with(&path, false, |pending| if pending {
                Err("unresolved recovery".into())
            } else {
                Ok(Some(false))
            })
            .is_err());
        b.open_with(&path, false, |_| Ok(Some(true))).unwrap();
        assert!(b.session.is_dirty());
        assert_eq!(b.session.project().screens[0].name, "Recover me");
        fs::write(&path, b"external changes").unwrap();
        assert!(b.save().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"external changes");
        assert!(b.session.is_dirty());
    }

    #[test]
    fn undo_to_saved_state_retires_recovery_and_redo_recreates_it() {
        let t = Temp::new();
        let path = t.0.join("undo.hmi.json");
        let mut a = Host::new(&t.0.join("app")).unwrap();
        a.save_as(&path).unwrap();
        edit(&mut a, "Unsaved edit");
        a.autosave();
        let envelope = a.active_recovery().unwrap();
        assert!(envelope.exists());
        a.session.undo(None).unwrap();
        assert!(!a.session.is_dirty());
        a.autosave();
        assert!(!envelope.exists());
        a.session.redo(None).unwrap();
        assert!(a.session.is_dirty());
        a.autosave();
        assert!(envelope.exists());
        drop(a);
        let mut b = Host::new(&t.0.join("app")).unwrap();
        assert!(b.open_with(&path, false, |_| Ok(Some(true))).unwrap());
        assert_eq!(b.session.project().screens[0].name, "Unsaved edit");
    }

    #[test]
    fn untitled_recovery_and_failed_open_preserve_session() {
        let t = Temp::new();
        let dir = t.0.join("app");
        let mut a = Host::new(&dir).unwrap();
        edit(&mut a, "Untitled");
        a.autosave();
        drop(a);
        let mut b = Host::new(&dir).unwrap();
        assert!(b.pending_untitled);
        assert!(b.save_as(&t.0.join("blocked.hmi.json")).is_err());
        b.recover_untitled().unwrap();
        let bad = t.0.join("bad.hmi.json");
        fs::write(&bad, b"invalid").unwrap();
        assert!(b.open_with(&bad, true, |_| Ok(Some(false))).is_err());
        assert_eq!(b.session.project().screens[0].name, "Untitled");
        assert!(b.save_as(&bad).is_err());
        assert_eq!(fs::read(&bad).unwrap(), b"invalid");
    }

    #[test]
    fn cancellation_and_new_project_preserve_or_discard_explicitly() {
        let t = Temp::new();
        let mut a = Host::new(&t.0.join("app")).unwrap();
        let path = t.0.join("candidate.hmi.json");
        let mut b = Host::new(&t.0.join("app-b")).unwrap();
        b.save_as(&path).unwrap();
        drop(b);
        edit(&mut a, "Current");
        a.autosave();
        assert!(!a.open_with(&path, true, |_| Ok(None)).unwrap());
        assert_eq!(a.generation, 0);
        assert!(a.session.is_dirty());
        assert!(a.active_recovery().unwrap().exists());
        assert!(a.open_with(&path, false, |_| Ok(Some(false))).is_err());
        assert!(a.new_project(false).is_err());
        a.new_project(true).unwrap();
        assert_eq!(a.generation, 1);
        assert!(!a.session.is_dirty());
        assert!(!a.active_recovery().unwrap().exists());
        assert!(a.open_with(&path, false, |_| Ok(Some(false))).unwrap());
        assert_eq!(a.generation, 2);
        assert_eq!(a.session.path().unwrap(), path.canonicalize().unwrap());
    }

    #[test]
    fn recover_deleted_project_and_save_as_new_file() {
        let t = Temp::new();
        let path = t.0.join("deleted.hmi.json");
        let mut a = Host::new(&t.0.join("app")).unwrap();
        a.save_as(&path).unwrap();
        edit(&mut a, "Recovered after deletion");
        a.autosave();
        let envelope = a.active_recovery().unwrap();
        fs::remove_file(&path).unwrap();
        drop(a);
        let mut b = Host::new(&t.0.join("app")).unwrap();
        let selected = recovery_project(&envelope).unwrap();
        assert!(b.open_with(&selected, false, |_| Ok(Some(true))).unwrap());
        assert!(b.save().is_err());
        let new_path = t.0.join("new.hmi.json");
        b.save_as(&new_path).unwrap();
        assert!(!path.exists());
        assert!(new_path.exists());
        assert!(!envelope.exists());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_project_and_sidecar() {
        let t = Temp::new();
        let mut a = Host::new(&t.0.join("app")).unwrap();
        let external = t.0.join("outside");
        fs::create_dir(&external).unwrap();
        let path = t.0.join("line.hmi.json");
        std::os::unix::fs::symlink(&external, sidecar_dir(&path)).unwrap();
        assert!(a.save_as(&path).is_err());
        assert!(!path.exists());
        assert_eq!(fs::read_dir(&external).unwrap().count(), 0);
        std::os::unix::fs::symlink(external.join("file"), &path).unwrap();
        assert!(a.save_as(&path).is_err());
    }
}
