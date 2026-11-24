//! Minimal filesystem abstraction used by persistence.
//!
//! Production code uses [`StdFileSystem`]. Tests substitute
//! [`FaultyFileSystem`] to simulate failed writes, interrupted saves and
//! failed renames deterministically. The trait is intentionally narrow: it
//! is not, and must never become, a generic filesystem bridge for the UI.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Filesystem operations needed for crash-safe project persistence.
pub trait FileSystem: Send + Sync {
    /// Reads a whole file.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    /// Exclusively creates `path`, writes `data`, flushes and syncs it to
    /// stable storage before returning.
    fn write_synced(&self, path: &Path, data: &[u8]) -> io::Result<()>;
    /// Atomically replaces `to` with `from` (same directory).
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Removes a file.
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    /// Creates a directory and its parents.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    /// Whether a file exists.
    fn exists(&self, path: &Path) -> bool;
    /// Lists file paths in a directory (unsorted).
    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    /// Makes a completed rename durable (fsyncs the directory where the
    /// platform supports it; no-op elsewhere).
    fn sync_dir(&self, path: &Path) -> io::Result<()>;
}

/// `std::fs`-backed implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct StdFileSystem;

impl FileSystem for StdFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn write_synced(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        f.write_all(data)?;
        f.flush()?;
        f.sync_all()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        // POSIX rename(2) atomically replaces `to`. On Windows std uses
        // MoveFileExW(MOVEFILE_REPLACE_EXISTING), which replaces an existing
        // file on the same volume; it fails (leaving `to` intact) if `to` is
        // read-only or open without FILE_SHARE_DELETE.
        std::fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn exists(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                out.push(entry.path());
            }
        }
        Ok(out)
    }

    #[cfg(unix)]
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        std::fs::File::open(path)?.sync_all()
    }

    #[cfg(not(unix))]
    fn sync_dir(&self, _path: &Path) -> io::Result<()> {
        // Windows has no portable directory fsync through std; NTFS journals
        // the rename metadata.
        Ok(())
    }
}

/// Which operation a [`FaultyFileSystem`] should break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// `write_synced` fails before writing anything.
    WriteFails,
    /// `write_synced` writes only the first half of the data, then fails
    /// (simulates a crash or full disk mid-write).
    WriteInterrupted,
    /// `write_synced` reports success but stores only half the data
    /// (simulates silent truncation / lying storage).
    WriteTruncatedSilently,
    /// `rename` fails.
    RenameFails,
}

/// Test filesystem that injects one fault into the N-th matching operation
/// whose path contains `path_filter`, delegating everything else to
/// [`StdFileSystem`].
#[derive(Debug)]
pub struct FaultyFileSystem {
    fault: Fault,
    path_filter: String,
    skip: AtomicUsize,
    fired: Mutex<bool>,
}

impl FaultyFileSystem {
    /// Breaks the first matching operation after skipping `skip` matches.
    pub fn new(fault: Fault, path_filter: impl Into<String>, skip: usize) -> Self {
        Self {
            fault,
            path_filter: path_filter.into(),
            skip: AtomicUsize::new(skip),
            fired: Mutex::new(false),
        }
    }

    /// Whether the fault has been injected.
    pub fn fired(&self) -> bool {
        self.fired.lock().map(|g| *g).unwrap_or(true)
    }

    fn should_fire(&self, path: &Path) -> bool {
        if !path.to_string_lossy().contains(&self.path_filter) {
            return false;
        }
        let Ok(mut fired) = self.fired.lock() else {
            return false;
        };
        if *fired {
            return false;
        }
        if self.skip.load(Ordering::SeqCst) > 0 {
            self.skip.fetch_sub(1, Ordering::SeqCst);
            return false;
        }
        *fired = true;
        true
    }
}

fn injected() -> io::Error {
    io::Error::other("injected fault")
}

impl FileSystem for FaultyFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        StdFileSystem.read(path)
    }

    fn write_synced(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        let is_write_fault = matches!(
            self.fault,
            Fault::WriteFails | Fault::WriteInterrupted | Fault::WriteTruncatedSilently
        );
        if is_write_fault && self.should_fire(path) {
            match self.fault {
                Fault::WriteFails => return Err(injected()),
                Fault::WriteInterrupted => {
                    StdFileSystem.write_synced(path, &data[..data.len() / 2])?;
                    return Err(injected());
                }
                Fault::WriteTruncatedSilently => {
                    return StdFileSystem.write_synced(path, &data[..data.len() / 2]);
                }
                Fault::RenameFails => {}
            }
        }
        StdFileSystem.write_synced(path, data)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.fault == Fault::RenameFails && self.should_fire(to) {
            return Err(injected());
        }
        StdFileSystem.rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        StdFileSystem.remove_file(path)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        StdFileSystem.create_dir_all(path)
    }

    fn exists(&self, path: &Path) -> bool {
        StdFileSystem.exists(path)
    }

    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        StdFileSystem.list_dir(path)
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        StdFileSystem.sync_dir(path)
    }
}
