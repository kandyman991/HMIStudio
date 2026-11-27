//! Minimal filesystem abstraction used by persistence.
//!
//! Production code uses [`StdFileSystem`]. Tests substitute
//! [`FaultyFileSystem`] to simulate failed writes, interrupted saves and
//! failed renames deterministically. The trait is intentionally narrow: it
//! is not, and must never become, a generic filesystem bridge for the UI.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Filesystem operations needed for crash-safe project persistence.
pub trait FileSystem: Send + Sync {
    /// Reads a whole file.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    /// Read at most limit + 1 bytes, refusing larger files before allocation.
    fn read_limited(&self, _path: &Path, _limit: usize) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no bounded read support",
        ))
    }
    /// Exclusively creates `path`, writes `data`, flushes and syncs it to
    /// stable storage before returning.
    fn write_synced(&self, path: &Path, data: &[u8]) -> io::Result<()>;
    /// Exclusively stages a review bundle that contains the exact legacy source.
    /// Unix implementations must create it with owner-only permissions.
    fn write_review_synced(&self, _path: &Path, _data: &[u8]) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no review staging support",
        ))
    }
    /// Atomically replaces `to` with `from` (same directory).
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Atomically publish a staged file only if the destination is absent.
    fn link_new(&self, _from: &Path, _to: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no exclusive publication support",
        ))
    }
    /// Removes a file.
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    /// Remove the exclusively created staging name after publication.
    fn unlink_staged(&self, path: &Path) -> io::Result<()> {
        self.remove_file(path)
    }
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

fn write_new_synced(path: &Path, data: &[u8], _owner_only: bool) -> io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if _owner_only {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(data)?;
    file.flush()?;
    file.sync_all()
}

impl FileSystem for StdFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn read_limited(&self, path: &Path, limit: usize) -> io::Result<Vec<u8>> {
        let file = std::fs::File::open(path)?;
        if file.metadata()?.len() > limit as u64 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "file too large"));
        }
        let mut bytes = Vec::new();
        file.take((limit as u64).saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() > limit {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "file too large"));
        }
        Ok(bytes)
    }

    fn write_synced(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        write_new_synced(path, data, false)
    }

    fn write_review_synced(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        write_new_synced(path, data, true)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        // POSIX rename(2) atomically replaces `to`. On Windows std uses
        // MoveFileExW(MOVEFILE_REPLACE_EXISTING), which replaces an existing
        // file on the same volume; it fails (leaving `to` intact) if `to` is
        // read-only or open without FILE_SHARE_DELETE.
        std::fs::rename(from, to)
    }

    fn link_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::hard_link(from, to)
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
    fn write_with_faults(&self, path: &Path, data: &[u8], owner_only: bool) -> io::Result<()> {
        let is_write_fault = matches!(
            self.fault,
            Fault::WriteFails | Fault::WriteInterrupted | Fault::WriteTruncatedSilently
        );
        if is_write_fault && self.should_fire(path) {
            match self.fault {
                Fault::WriteFails => return Err(injected()),
                Fault::WriteInterrupted => {
                    write_new_synced(path, &data[..data.len() / 2], owner_only)?;
                    return Err(injected());
                }
                Fault::WriteTruncatedSilently => {
                    return write_new_synced(path, &data[..data.len() / 2], owner_only);
                }
                Fault::RenameFails => {}
            }
        }
        write_new_synced(path, data, owner_only)
    }
}

fn injected() -> io::Error {
    io::Error::other("injected fault")
}

impl FileSystem for FaultyFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        StdFileSystem.read(path)
    }

    fn read_limited(&self, path: &Path, limit: usize) -> io::Result<Vec<u8>> {
        StdFileSystem.read_limited(path, limit)
    }

    fn write_synced(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        self.write_with_faults(path, data, false)
    }

    fn write_review_synced(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        self.write_with_faults(path, data, true)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.fault == Fault::RenameFails && self.should_fire(to) {
            return Err(injected());
        }
        StdFileSystem.rename(from, to)
    }

    fn link_new(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.fault == Fault::RenameFails && self.should_fire(to) {
            return Err(injected());
        }
        StdFileSystem.link_new(from, to)
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
