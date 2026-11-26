//! Native persistence boundary. The webview never supplies a filesystem path.
use hmi_engineering_core::{
    fs::{FileSystem, StdFileSystem},
    legacy_quarantine::{requires_review, QuarantinedImport, MAX_LEGACY_SOURCE_BYTES},
    persistence::sidecar_dir,
    AutosaveOutcome, ProjectSession, ProjectSnapshot, SaveOptions, SessionConfig,
};
use hmi_project_model::{load_project, model::Project, MigrationRegistry};
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
    safe_regular_file(path, false)
}

fn safe_regular_file(path: &Path, _allow_hard_links: bool) -> Result<bool> {
    safe_ancestors(path)?;
    match metadata(path)? {
        Some(m) if m.is_file() => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if !_allow_hard_links && m.nlink() != 1 {
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

fn is_review_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".hmi-review")
}

fn normalize_review(path: &Path) -> Result<PathBuf> {
    safe_regular_file(path, true)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid review bundle filename")?;
    if name.contains(':') || !is_review_name(name) {
        return Err("Select an .hmi-review bundle".into());
    }
    let parent = path.parent().ok_or("Missing bundle directory")?;
    let target = parent.canonicalize().map_err(err)?.join(name);
    safe_regular_file(&target, true)?;
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
        // A crash may leave both names from exclusive review publication.
        // Opening this immutable envelope checks its full contents and never
        // writes through either name. All mutating paths still use safe_file.
        let review = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_review_name);
        safe_regular_file(path, review).map_err(io::Error::other)?;
        StdFileSystem.read_limited(path, limit)
    }
    fn write_synced(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        safe_file(path).map_err(io::Error::other)?;
        StdFileSystem.write_synced(path, data)
    }
    fn write_review_synced(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        safe_file(path).map_err(io::Error::other)?;
        StdFileSystem.write_review_synced(path, data)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        safe_file(from).map_err(io::Error::other)?;
        safe_file(to).map_err(io::Error::other)?;
        StdFileSystem.rename(from, to)
    }
    fn link_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        safe_file(from).map_err(io::Error::other)?;
        if safe_file(to).map_err(io::Error::other)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "bundle exists",
            ));
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

enum ActiveProject {
    Ordinary(ProjectSession),
    Review {
        bundle: Box<QuarantinedImport>,
        recovery_retained: bool,
    },
}

/// Borrowed from the active native session. Production consumers must take
/// this type and apply their own authorization before performing live work.
#[cfg_attr(not(test), allow(dead_code))] // No production consumer yet.
pub struct ProductionCandidate<'a> {
    project: &'a Project,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ProductionCandidate<'_> {
    pub fn project(&self) -> &Project {
        self.project
    }
}

pub struct Host {
    active: ActiveProject,
    runtime: crate::runtime_host::RuntimeHost,
    pub generation: u64,
    untitled: PathBuf,
    _instance_lock: File,
    project_lock: Option<File>,
    pub pending_untitled: bool,
    autosave_message: String,
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
            active: ActiveProject::Ordinary(ProjectSession::with_fs(
                project,
                config(dir),
                Arc::new(GuardedFileSystem),
            )),
            runtime: crate::runtime_host::RuntimeHost::new(
                std::env::var_os("HMI_STUDIO_SIM_ENGINE").map(PathBuf::from),
            ),
            generation: 0,
            untitled: dir.to_path_buf(),
            _instance_lock: instance_lock,
            project_lock: None,
            pending_untitled: pending,
            autosave_message: "Autosave ready".into(),
        })
    }

    pub fn is_review(&self) -> bool {
        matches!(self.active, ActiveProject::Review { .. })
    }

    pub fn is_dirty(&self) -> bool {
        match &self.active {
            ActiveProject::Ordinary(session) => session.is_dirty(),
            ActiveProject::Review { bundle, .. } => bundle.is_dirty(),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match &self.active {
            ActiveProject::Ordinary(session) => session.path(),
            ActiveProject::Review { bundle, .. } => Some(bundle.path()),
        }
    }

    pub fn snapshot(&self) -> ProjectSnapshot {
        match &self.active {
            ActiveProject::Ordinary(session) => session.snapshot(),
            ActiveProject::Review { bundle, .. } => bundle.snapshot(),
        }
    }

    pub fn persistence_message(&self) -> String {
        match &self.active {
            ActiveProject::Ordinary(_) => self.autosave_message.clone(),
            ActiveProject::Review {
                bundle,
                recovery_retained,
            } => {
                let mut message = review_message(bundle);
                if *recovery_retained {
                    message.push_str(". Previous recovery retained");
                }
                message
            }
        }
    }

    pub fn request(
        &mut self,
        request: crate::session_host::WorkspaceRequest,
        generation: Option<u64>,
    ) -> Result<crate::session_host::WorkspaceReply> {
        request.check_context(generation, self.generation, self.pending_untitled)?;
        let reply = match &mut self.active {
            ActiveProject::Ordinary(session) => crate::session_host::request(session, request),
            ActiveProject::Review { bundle, .. } => {
                crate::session_host::review_request(bundle, request)
            }
        };
        self.runtime.invalidate_on_edit(reply.revision());
        Ok(reply)
    }

    pub fn simulate(
        &mut self,
        generation: u64,
        request: crate::runtime_host::RuntimeRequest,
        confirm: impl FnOnce() -> bool,
    ) -> Result<Option<crate::runtime_host::RuntimeReply>> {
        use crate::runtime_host::RuntimeRequest;
        self.check_generation(generation)?;
        if self.pending_untitled {
            return Err("Resolve untitled recovery before starting simulation".into());
        }
        if let RuntimeRequest::Start { revision } = request {
            let snapshot = self.snapshot();
            if revision != snapshot.revision {
                return Err("Project changed. Refresh before starting simulation".into());
            }
            return self.runtime.start(&snapshot).map(Some);
        }
        self.runtime.run(request, confirm)
    }

    /// Translate native state, not error-message wording, into IPC outcomes.
    pub fn simulation_outcome(
        &mut self,
        generation: u64,
        request: crate::runtime_host::RuntimeRequest,
        confirm: impl FnOnce() -> bool,
    ) -> crate::runtime_host::RuntimeOutcome {
        use crate::runtime_host::{RuntimeOutcome, RuntimeRequest};
        if let Err(message) = self.check_generation(generation) {
            return RuntimeOutcome::Lost { message };
        }
        if self.pending_untitled {
            return RuntimeOutcome::Lost {
                message: "Resolve untitled recovery before starting simulation".into(),
            };
        }
        if let RuntimeRequest::Start { revision } = &request {
            if *revision != self.snapshot().revision {
                return RuntimeOutcome::Lost {
                    message: "Project changed. Refresh before starting simulation".into(),
                };
            }
        }
        let session = request.session();
        match self.simulate(generation, request, confirm) {
            Ok(Some(simulation)) => RuntimeOutcome::Active { simulation },
            Ok(None) => RuntimeOutcome::Stopped,
            Err(message) => match self.runtime.refresh(session) {
                Ok(Some(simulation)) => RuntimeOutcome::Rejected {
                    message,
                    simulation: Some(simulation),
                },
                Ok(None) if session.is_none() => RuntimeOutcome::Rejected {
                    message,
                    simulation: None,
                },
                Ok(None) => RuntimeOutcome::Lost { message },
                Err(error) => RuntimeOutcome::Lost {
                    message: format!("Simulation session lost: {error}"),
                },
            },
        }
    }

    pub fn stop_simulation(&mut self) {
        self.runtime.stop();
    }

    pub fn check_generation(&self, generation: u64) -> Result<()> {
        if generation != self.generation {
            return Err("Project session changed. Refresh before continuing".into());
        }
        Ok(())
    }

    /// Import-specific screening before a future production or live consumer
    /// may obtain the borrowed project. No actual consumer exists yet.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn production_candidate(&self) -> Result<ProductionCandidate<'_>> {
        let ActiveProject::Ordinary(session) = &self.active else {
            return Err("Production/live admission blocked: review required".into());
        };
        if self.pending_untitled || requires_review(session.project()) {
            return Err("Production/live admission blocked: review or recovery required".into());
        }
        if session.report().has_errors() {
            return Err("Production/live admission blocked: invalid project".into());
        }
        Ok(ProductionCandidate {
            project: session.project(),
        })
    }

    fn preflight(&self) -> Result<()> {
        if let ActiveProject::Ordinary(session) = &self.active {
            safe_sidecar(&self.untitled)?;
            if let Some(path) = session.path() {
                safe_file(path)?;
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
        if opened.session.project().has_v2_features() && opened.report.has_errors() {
            return Err("Invalid v2 project; original data preserved".into());
        }
        self.active = ActiveProject::Ordinary(opened.session);
        self.project_replaced();
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
        if opened.session.project().has_v2_features() && opened.report.has_errors() {
            return Err("Invalid v2 project; original data preserved".into());
        }
        if has_recovery && !recover {
            fs::remove_file(dir.join(RECOVERY)).map_err(err)?;
        }
        // The old writer lock is still held. Remove its recovery only after
        // the new project has been fully parsed and explicitly accepted.
        let recovery_retained = self.clear_active_recovery().is_err();
        self.active = ActiveProject::Ordinary(opened.session);
        self.project_replaced();
        self.project_lock = Some(guard);
        self.autosave_message = if recovery_retained {
            "Autosave ready. Previous recovery retained".into()
        } else {
            "Autosave ready".into()
        };
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
        safe_sidecar(&self.untitled)?;
        let pending_untitled = self.path().is_some() && safe_file(&self.untitled.join(RECOVERY))?;
        self.clear_active_recovery()?;
        self.active = ActiveProject::Ordinary(ProjectSession::with_fs(
            project,
            config(&self.untitled),
            Arc::new(GuardedFileSystem),
        ));
        self.pending_untitled = pending_untitled;
        self.project_replaced();
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
        let recovery_retained = self.clear_active_recovery().is_err()
            || matches!(
                self.active,
                ActiveProject::Review {
                    recovery_retained: true,
                    ..
                }
            );
        self.active = ActiveProject::Review {
            bundle: Box::new(candidate),
            recovery_retained,
        };
        self.project_lock = None;
        self.project_replaced();
    }

    fn project_replaced(&mut self) {
        self.stop_simulation();
        self.generation += 1;
    }

    pub fn check_import_source(&self, source: &Path) -> Result<()> {
        if !safe_file(source)? {
            return Err("Legacy source does not exist".into());
        }
        if !self.is_review() {
            let source = source.canonicalize().map_err(err)?;
            let recovery = self.active_recovery()?;
            if self.path().and_then(|p| p.canonicalize().ok()).as_ref() == Some(&source)
                || recovery.canonicalize().ok().as_ref() == Some(&source)
            {
                return Err(
                    "Select a legacy source outside the active project and its recovery slot"
                        .into(),
                );
            }
        }
        Ok(())
    }

    pub fn import_review(&mut self, source: &Path, target: &Path, discard: bool) -> Result<()> {
        self.prepare_review_switch(discard)?;
        self.check_import_source(source)?;
        let target = normalize_review(target)?;
        if safe_regular_file(&target, true)? {
            return Err("Choose a new review bundle filename".into());
        }
        let fs = Arc::new(GuardedFileSystem);
        let bytes = StdFileSystem
            .read_limited(source, MAX_LEGACY_SOURCE_BYTES)
            .map_err(err)?;
        let candidate = QuarantinedImport::import_new(&bytes, &target, config(&self.untitled), fs)
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
        let ActiveProject::Ordinary(session) = &mut self.active else {
            return Err("Review bundles are immutable; save a new review copy".into());
        };
        let outcome = session.save(SaveOptions::default()).map_err(err)?;
        self.autosave_message = save_message(&outcome.warnings);
        Ok(())
    }

    pub fn save_as(&mut self, path: &Path) -> Result<()> {
        self.preflight()?;
        if self.pending_untitled {
            return Err("Resolve untitled recovery first".into());
        }
        let session = match &mut self.active {
            ActiveProject::Ordinary(session) => session,
            ActiveProject::Review { bundle, .. } => {
                let target = normalize_review(path)?;
                if safe_regular_file(&target, true)? {
                    return Err("Choose a new review bundle filename".into());
                }
                return bundle.save_copy(&target).map_err(err);
            }
        };
        let path = normalize_project(path)?;
        if session.path() == Some(path.as_path()) {
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
        let outcome = session
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
        let preflight = self.preflight();
        if let ActiveProject::Ordinary(session) = &mut self.active {
            self.autosave_message = match preflight.and_then(|_| session.autosave().map_err(err)) {
                Ok(AutosaveOutcome::Clean) => "No unsaved edits to recover".into(),
                Ok(_) => "Recovery snapshot current".into(),
                Err(e) => format!("Autosave failed: {e}"),
            };
        }
    }

    // Discard only the active project's recovery after explicit confirmation.
    // Call only after a successful candidate open or immediately before exit.
    pub fn clear_active_recovery(&self) -> Result<()> {
        if self.is_review() {
            return Ok(());
        }
        remove_recovery(&self.active_recovery()?)
    }

    fn active_recovery(&self) -> Result<PathBuf> {
        let ActiveProject::Ordinary(session) = &self.active else {
            return Err("Review workspaces have no ordinary recovery slot".into());
        };
        self.preflight()?;
        Ok(session
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
        message.push_str(". ");
        message.push_str(&review.warnings().join(". "));
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
    const LEGACY_SOURCE: &[u8] = br#"{"schemaVersion":1,"metadata":{"name":"Synthetic","defaultScreenId":"screen/a","targetProfiles":[]},"screens":{"screen/a":{"id":"screen/a","title":"Main","widthDp":640,"heightDp":480,"components":{"label-a":{"id":"label-a","type":"label","properties":{"text":"Synthetic"}}}}},"variables":{},"externalSources":{},"externalMappings":{}}"#;

    impl Host {
        fn ordinary(&self) -> &ProjectSession {
            let ActiveProject::Ordinary(session) = &self.active else {
                panic!("ordinary session expected")
            };
            session
        }

        fn ordinary_mut(&mut self) -> &mut ProjectSession {
            let ActiveProject::Ordinary(session) = &mut self.active else {
                panic!("ordinary session expected")
            };
            session
        }
    }

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
        let screen = host.ordinary().project().screens[0].id.clone();
        host.ordinary_mut()
            .execute(Transaction::single(Command::RenameScreen {
                screen,
                name: name.into(),
            }))
            .unwrap();
    }

    fn simulation_legacy_source() -> Vec<u8> {
        use serde_json::json;

        let mut legacy: serde_json::Value = serde_json::from_slice(LEGACY_SOURCE).unwrap();
        legacy["schemaVersion"] = json!(9);
        legacy["variables"] = json!({
            "count":{"id":"count","path":"Count","type":"LONG","access":"SETPOINT"},
            "pulse":{"id":"pulse","path":"Pulse","type":"BOOL","access":"COMMAND"},
            "input":{"id":"input","path":"Input","type":"BOOL","access":"READ_ONLY"}
        });
        legacy["externalSources"] = json!({
            "physical":{"id":"physical","driver":"s7","configuration":{
                "host":"example.invalid","port":"102","rack":"0","slot":"1","pollIntervalMs":"200","timeoutMs":"2000"
            },"commissioningAllowed":true}
        });
        legacy["externalMappings"] = json!({
            "write":{"id":"write","internalVariableId":"count","sourceId":"physical","externalAddress":"DB1.DINT0",
                "access":"READ_WRITE","writeConfirmed":true,"command":{"kind":"SETPOINT","min":0,"max":100,"step":2,"confirm":true}},
            "pulse":{"id":"pulse","internalVariableId":"pulse","sourceId":"physical","externalAddress":"DB1.X4.0",
                "access":"READ_WRITE","writeConfirmed":true,"command":{"kind":"MOMENTARY","pulseMs":1000}},
            "input":{"id":"input","internalVariableId":"input","sourceId":"physical","externalAddress":"DB1.X5.0","access":"READ"}
        });
        serde_json::to_vec(&legacy).unwrap()
    }

    #[test]
    fn simulation_fixture_imports_as_quarantined_canonical_project() {
        let t = Temp::new();
        let source = t.0.join("simulation.json");
        let review = t.0.join("simulation.hmi-review");
        fs::write(&source, simulation_legacy_source()).unwrap();
        let mut host = Host::new(&t.0.join("app")).unwrap();
        host.import_review(&source, &review, false).unwrap();
        let project = host.snapshot().project;
        assert_eq!(project.tags.len(), 3);
        assert_eq!(project.connections.len(), 1);
        assert_eq!(project.v2.industrial.tags.len(), 2);
        assert!(host.production_candidate().is_err());
    }

    #[test]
    #[ignore = "requires freshly built hmi-engine selected by hosted CI"]
    fn native_project_simulation_reports_child_loss_and_restarts() {
        use crate::runtime_host::{RuntimeHost, RuntimeOutcome, RuntimeRequest as Request};
        use std::process::Command;
        use std::time::{Duration, Instant};

        let t = Temp::new();
        let source = t.0.join("simulation.json");
        let review = t.0.join("simulation.hmi-review");
        fs::write(&source, simulation_legacy_source()).unwrap();
        let mut host = Host::new(&t.0.join("app")).unwrap();
        host.runtime = RuntimeHost::new(Some(PathBuf::from(
            std::env::var_os("HMI_ENGINE_TEST_BIN").expect("CI must provide a fresh engine"),
        )));
        host.import_review(&source, &review, false).unwrap();
        let generation = host.generation;
        let revision = host.snapshot().revision;
        let start = || Request::Start { revision };
        let RuntimeOutcome::Active { simulation: first } =
            host.simulation_outcome(generation, start(), || false)
        else {
            panic!("simulation did not start");
        };
        let session = first.session;
        let count = first
            .points
            .iter()
            .find(|point| point.name == "Count")
            .unwrap()
            .id
            .clone();
        let write = |value: &str| Request::Write {
            session,
            tag: count.clone(),
            value: value.into(),
        };
        for (value, confirmed) in [("101", true), ("24", false)] {
            assert!(matches!(
                host.simulation_outcome(generation, write(value), || confirmed),
                RuntimeOutcome::Rejected {
                    simulation: Some(reply),
                    ..
                } if reply.session == session
            ));
        }
        assert!(matches!(
            host.simulation_outcome(generation, write("24"), || true),
            RuntimeOutcome::Active { .. }
        ));
        let read = || Request::Read { session };
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            let RuntimeOutcome::Active { simulation } =
                host.simulation_outcome(generation, read(), || false)
            else {
                panic!("simulation lost before forced exit");
            };
            if simulation
                .points
                .iter()
                .any(|p| p.id == count && p.value.as_deref() == Some("24"))
            {
                break;
            }
            assert!(Instant::now() < deadline, "write not observed");
            std::thread::sleep(Duration::from_millis(10));
        }

        // Kill only the direct, test-owned real engine, without using the
        // supervisor's normal shutdown path.
        let pid = host.runtime.child_process_id().to_string();
        #[cfg(windows)]
        let status = Command::new("taskkill")
            .args(["/PID", &pid, "/F"])
            .status()
            .unwrap();
        #[cfg(unix)]
        let status = Command::new("kill").args(["-KILL", &pid]).status().unwrap();
        assert!(status.success());
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            match host.simulation_outcome(generation, read(), || false) {
                RuntimeOutcome::Lost { .. } => break,
                RuntimeOutcome::Active { .. } => {}
                _ => panic!("engine exit was not classified as session loss"),
            }
            assert!(Instant::now() < deadline, "engine exit not detected");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(matches!(
            host.simulation_outcome(generation, write("24"), || true),
            RuntimeOutcome::Lost { .. }
        ));
        let RuntimeOutcome::Active { simulation: second } =
            host.simulation_outcome(generation, start(), || false)
        else {
            panic!("fresh simulation did not start");
        };
        assert_ne!(second.session, session);
        assert!(second
            .points
            .iter()
            .any(|p| p.id == count && p.value.as_deref() == Some("0")));
        assert!(matches!(
            host.simulation_outcome(generation, read(), || false),
            RuntimeOutcome::Lost { .. }
        ));
        assert!(matches!(
            host.simulation_outcome(
                generation,
                Request::Read {
                    session: second.session
                },
                || false,
            ),
            RuntimeOutcome::Active { .. }
        ));
        assert!(host.production_candidate().is_err());
    }

    #[test]
    #[ignore = "requires freshly built hmi-engine selected by hosted CI"]
    fn native_project_simulation_respects_quarantine_and_session_lifecycle() {
        use crate::runtime_host::{RuntimeHost, RuntimeReply, RuntimeRequest as Request};
        use crate::session_host::WorkspaceRequest;
        use std::time::{Duration, Instant};

        let t = Temp::new();
        let source = t.0.join("simulation.json");
        let review = t.0.join("simulation.hmi-review");
        let original = simulation_legacy_source();
        fs::write(&source, &original).unwrap();
        let mut host = Host::new(&t.0.join("app")).unwrap();
        host.runtime = RuntimeHost::new(Some(PathBuf::from(
            std::env::var_os("HMI_ENGINE_TEST_BIN").expect("CI must provide a fresh engine"),
        )));
        host.import_review(&source, &review, false).unwrap();
        let before = host.snapshot();
        let generation = host.generation;
        assert!(host.is_review());
        assert!(host.production_candidate().is_err());
        let start = || Request::Start {
            revision: before.revision,
        };
        let first = host
            .simulate(generation, start(), || false)
            .unwrap()
            .unwrap();
        let id = |name: &str| {
            first
                .points
                .iter()
                .find(|p| p.name == name)
                .unwrap()
                .id
                .clone()
        };
        let count = id("Count");
        let pulse = id("Pulse");
        let input = id("Input");
        let session = first.session;
        let read = || Request::Read { session };
        let write = |tag: &str, value: &str| Request::Write {
            session,
            tag: tag.into(),
            value: value.into(),
        };
        let value = |reply: &RuntimeReply, id: &str| {
            reply
                .points
                .iter()
                .find(|p| p.id == id)
                .unwrap()
                .value
                .clone()
        };
        let wait_for = |host: &mut Host, id: &str, expected: &str| {
            let deadline = Instant::now() + Duration::from_secs(4);
            loop {
                let reply = host
                    .simulate(generation, read(), || false)
                    .unwrap()
                    .unwrap();
                if value(&reply, id).as_deref() == Some(expected) {
                    break;
                }
                assert!(Instant::now() < deadline, "expected {expected} for {id}");
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        assert_eq!(value(&first, &count).as_deref(), Some("0"));
        assert!(host
            .simulate(generation, write(&input, "true"), || true)
            .is_err());
        assert!(host
            .simulate(generation, write(&count, "101"), || true)
            .is_err());
        assert!(host
            .simulate(generation, write(&count, "25"), || true)
            .is_err());
        assert!(host
            .simulate(generation, write(&count, "24"), || false)
            .is_err());
        host.simulate(generation, write(&count, "24"), || true)
            .unwrap();
        wait_for(&mut host, &count, "24");
        assert!(host
            .simulate(generation, write(&pulse, "true"), || true)
            .is_err());
        host.simulate(
            generation,
            Request::Pulse {
                session,
                tag: pulse.clone(),
            },
            || true,
        )
        .unwrap();
        wait_for(&mut host, &pulse, "true");
        wait_for(&mut host, &pulse, "false");
        assert_eq!(host.snapshot(), before);
        assert_eq!(fs::read(&source).unwrap(), original);
        assert!(host.production_candidate().is_err());
        assert!(host.simulate(generation, start(), || true).is_err());

        // Failed switching leaves the original runtime available. Committed
        // edits and successful switches retire it, including same-project reopen.
        assert!(host
            .open_review(&t.0.join("absent.hmi-review"), false)
            .is_err());
        wait_for(&mut host, &count, "24");
        host.request(
            WorkspaceRequest::Execute {
                transaction: Transaction::single(Command::RenameScreen {
                    screen: before.project.screens[0].id.clone(),
                    name: "Edited".into(),
                })
                .at_revision(before.revision),
            },
            Some(generation),
        )
        .unwrap();
        assert!(host.simulate(generation, read(), || true).is_err());
        assert!(host.simulate(generation, start(), || true).is_err());
        let second = host
            .simulate(
                generation,
                Request::Start {
                    revision: host.snapshot().revision,
                },
                || false,
            )
            .unwrap()
            .unwrap();
        assert_ne!(second.session, session);
        assert_eq!(value(&second, &count).as_deref(), Some("0"));
        assert!(host
            .simulate(generation, write(&count, "24"), || true)
            .is_err());
        host.new_project(true).unwrap();
        assert!(host
            .simulate(
                generation,
                Request::Read {
                    session: second.session
                },
                || true
            )
            .is_err());
        assert!(host
            .simulate(
                host.generation,
                Request::Read {
                    session: second.session
                },
                || true
            )
            .is_err());
        host.open_review(&review, false).unwrap();
        let reopened = host
            .simulate(host.generation, start(), || false)
            .unwrap()
            .unwrap();
        assert_ne!(reopened.session, second.session);
        assert_eq!(value(&reopened, &count).as_deref(), Some("0"));
        assert!(host.production_candidate().is_err());
    }

    #[test]
    fn native_review_lifecycle_blocks_plain_persistence_and_production() {
        use crate::session_host::WorkspaceRequest;

        let t = Temp::new();
        let source = t.0.join("legacy.json");
        let target = t.0.join("legacy.hmi-review");
        let copy = t.0.join("edited.hmi-review");
        let original = LEGACY_SOURCE;
        fs::write(&source, original).unwrap();
        let mut host = Host::new(&t.0.join("app")).unwrap();
        assert_eq!(
            host.production_candidate().unwrap().project(),
            host.ordinary().project()
        );
        let previous_generation = host.generation;
        host.import_review(&source, &target, false).unwrap();
        assert!(host.is_review());
        assert!(host.production_candidate().is_err());
        assert_eq!(fs::read(&source).unwrap(), original);
        let before = host.snapshot();
        let screen = before.project.screens[0].id.clone();
        let rename = Transaction::single(Command::RenameScreen {
            screen,
            name: "Reviewed edit".into(),
        });
        assert!(host.check_generation(previous_generation).is_err());
        assert!(host
            .request(
                WorkspaceRequest::Execute {
                    transaction: rename.clone().at_revision(before.revision),
                },
                Some(previous_generation)
            )
            .is_err());
        assert!(host
            .request(
                WorkspaceRequest::Execute {
                    transaction: rename.clone(),
                },
                Some(host.generation)
            )
            .is_err());
        let _ = host
            .request(
                WorkspaceRequest::Execute {
                    transaction: Transaction::single(Command::SetExtension {
                        key: "hmistudio.importQuarantine".into(),
                        value: None,
                    })
                    .at_revision(before.revision),
                },
                Some(host.generation),
            )
            .unwrap();
        assert_eq!(host.snapshot(), before);
        let _ = host
            .request(
                WorkspaceRequest::Execute {
                    transaction: rename.at_revision(before.revision),
                },
                Some(host.generation),
            )
            .unwrap();
        assert!(host.is_dirty());
        assert!(host.new_project(false).is_err());
        assert!(host.save().is_err());
        assert!(host.save_as(&source).is_err());
        assert!(host.save_as(&t.0.join("plain.hmi.json")).is_err());
        assert!(host.production_candidate().is_err());
        host.autosave();
        assert!(!host.untitled.join(RECOVERY).exists());
        assert!(host.persistence_message().contains("no autosave"));
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
        fs::write(
            &stripped,
            hmi_project_model::save_project(&project).unwrap(),
        )
        .unwrap();
        assert!(host
            .open_with(&stripped, false, |_| Ok(Some(false)))
            .is_err());
        assert!(host.is_review());
        assert_eq!(fs::read(&source).unwrap(), original);
    }

    #[test]
    fn refused_import_keeps_dirty_session_and_recovery_intact() {
        let t = Temp::new();
        let mut host = Host::new(&t.0.join("app")).unwrap();
        edit(&mut host, "Pending edit");
        host.autosave();
        let recovery = host.active_recovery().unwrap();
        let bytes = fs::read(&recovery).unwrap();
        let source = t.0.join("refused.json");
        fs::write(&source, b"{}").unwrap();
        let target = t.0.join("refused.hmi-review");
        let before = host.snapshot();
        let generation = host.generation;
        assert!(host.import_review(&source, &target, true).is_err());
        assert!(host.import_review(&recovery, &target, true).is_err());
        assert_eq!(host.snapshot(), before);
        assert_eq!(host.generation, generation);
        assert_eq!(fs::read(recovery).unwrap(), bytes);
        assert_eq!(fs::read(source).unwrap(), b"{}");
        assert!(!target.exists());
    }

    #[test]
    fn published_review_reopens_after_interrupted_staging_cleanup() {
        let t = Temp::new();
        let source = t.0.join("legacy.json");
        let target = t.0.join("review.hmi-review");
        let staged = t.0.join(".review.hmi-review.crash.tmp");
        let copy = t.0.join("copy.hmi-review");
        fs::write(&source, LEGACY_SOURCE).unwrap();
        {
            let mut host = Host::new(&t.0.join("app")).unwrap();
            host.import_review(&source, &target, false).unwrap();
        }
        let bytes = fs::read(&target).unwrap();
        fs::hard_link(&target, &staged).unwrap();
        let mut host = Host::new(&t.0.join("app")).unwrap();
        assert!(host.open_review(&staged, false).is_err());
        host.open_review(&target, false).unwrap();
        assert!(host.production_candidate().is_err());
        assert!(host.save_as(&target).is_err());
        host.save_as(&copy).unwrap();
        assert_eq!(fs::read(target).unwrap(), bytes);
        assert_eq!(fs::read(staged).unwrap(), bytes);
        assert_eq!(fs::read(source).unwrap(), LEGACY_SOURCE);
    }

    #[test]
    fn new_workspace_protects_an_untitled_slot_retained_during_review() {
        let t = Temp::new();
        let source = t.0.join("legacy.json");
        let app = t.0.join("app");
        fs::write(&source, LEGACY_SOURCE).unwrap();
        let mut host = Host::new(&app).unwrap();
        edit(&mut host, "Retained edit");
        host.autosave();
        let recovery = app.join(RECOVERY);
        let bytes = fs::read(&recovery).unwrap();
        host.import_review(&source, &t.0.join("review.hmi-review"), true)
            .unwrap();
        fs::write(&recovery, &bytes).unwrap(); // Model a cleanup failure after publication.
        host.new_project(false).unwrap();
        assert!(host.pending_untitled);
        assert!(host.production_candidate().is_err());
        host.autosave();
        assert_eq!(fs::read(recovery).unwrap(), bytes);
        host.recover_untitled().unwrap();
        assert_eq!(host.snapshot().project.screens[0].name, "Retained edit");
    }

    #[test]
    fn v2_project_opens_edits_saves_and_recovers_without_losing_declarations() {
        let t = Temp::new();
        let path = t.0.join("v2.hmi.json");
        let mut host = Host::new(&t.0.join("app")).unwrap();
        let mut project = host.ordinary().project().clone();
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
        host.ordinary_mut()
            .execute(Transaction::single(command))
            .unwrap();
        host.save().unwrap();
        let saved = host.ordinary().project().clone();
        assert_eq!(saved.v2.industrial, project.v2.industrial);
        assert_eq!(saved.extensions, project.extensions);
        edit(&mut host, "Recovered v2");
        host.autosave();
        drop(host);
        let mut reopened = Host::new(&t.0.join("app-b")).unwrap();
        assert!(reopened
            .open_with(&path, false, |_| Ok(Some(true)))
            .unwrap());
        assert_eq!(reopened.ordinary().project().v2, saved.v2);
        assert_eq!(reopened.ordinary().project().extensions, saved.extensions);
        assert_eq!(
            reopened.ordinary().project().screens[0].name,
            "Recovered v2"
        );
        reopened.save().unwrap();
        assert_eq!(
            hmi_project_model::load_project(&fs::read_to_string(&path).unwrap())
                .unwrap()
                .project,
            *reopened.ordinary().project()
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
        assert!(!a.is_dirty());
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
        assert!(b.is_dirty());
        assert_eq!(b.ordinary().project().screens[0].name, "Recover me");
        fs::write(&path, b"external changes").unwrap();
        assert!(b.save().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"external changes");
        assert!(b.is_dirty());
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
        a.ordinary_mut().undo(None).unwrap();
        assert!(!a.is_dirty());
        a.autosave();
        assert!(!envelope.exists());
        a.ordinary_mut().redo(None).unwrap();
        assert!(a.is_dirty());
        a.autosave();
        assert!(envelope.exists());
        drop(a);
        let mut b = Host::new(&t.0.join("app")).unwrap();
        assert!(b.open_with(&path, false, |_| Ok(Some(true))).unwrap());
        assert_eq!(b.ordinary().project().screens[0].name, "Unsaved edit");
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
        assert_eq!(b.ordinary().project().screens[0].name, "Untitled");
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
        assert!(a.is_dirty());
        assert!(a.active_recovery().unwrap().exists());
        assert!(a.open_with(&path, false, |_| Ok(Some(false))).is_err());
        assert!(a.new_project(false).is_err());
        a.new_project(true).unwrap();
        assert_eq!(a.generation, 1);
        assert!(!a.is_dirty());
        assert!(!a.active_recovery().unwrap().exists());
        assert!(a.open_with(&path, false, |_| Ok(Some(false))).unwrap());
        assert_eq!(a.generation, 2);
        assert_eq!(a.path().unwrap(), path.canonicalize().unwrap());
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
