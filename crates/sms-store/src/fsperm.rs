//! Owner-only permissions for the plaintext SMS database (ARCHITECTURE §4:
//! Linux DB mode 0600). SQLite creates `-wal`, `-shm` and `-journal` files
//! with the main database file's mode, so the database file is created (or
//! tightened) as 0600 *before* SQLite opens it; existing side files from an
//! older build are tightened too. The app directory is created as 0700, and
//! an existing app-owned directory is tightened to 0700.
//!
//! Non-unix targets: no-ops (the app ships for Linux and Android only).

use std::io;
use std::path::Path;

/// Side files SQLite keeps next to the database.
pub const SIDE_SUFFIXES: [&str; 3] = ["-wal", "-shm", "-journal"];

#[cfg(unix)]
mod imp {
    use std::fs::{self, DirBuilder, OpenOptions, Permissions};
    use std::io;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
    use std::path::Path;

    /// The process' uid without `unsafe`/libc: `/proc/self` is owned by it.
    /// Only paths owned by this uid are tightened.
    fn my_uid() -> Option<u32> {
        fs::metadata("/proc/self").ok().map(|m| m.uid())
    }

    fn tighten(path: &Path, mode: u32) -> io::Result<()> {
        let meta = fs::symlink_metadata(path)?;
        if meta.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "refusing to follow a symlink for the database path",
            ));
        }
        if let Some(uid) = my_uid()
            && meta.uid() != uid
        {
            // Not ours: leave it alone (and let SQLite fail if unusable).
            return Ok(());
        }
        if meta.mode() & 0o7777 != mode {
            fs::set_permissions(path, Permissions::from_mode(mode))?;
        }
        Ok(())
    }

    pub fn create_private_dir(dir: &Path) -> io::Result<()> {
        if let Some(parent) = dir.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        match DirBuilder::new().mode(0o700).create(dir) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        // The umask may have narrowed the mode further (fine) but never
        // widened it; an existing directory from an older build is tightened.
        tighten(dir, 0o700)
    }

    pub fn prepare_private_db(db: &Path) -> io::Result<()> {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(db)
        {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        tighten(db, 0o600)?;
        for suffix in super::SIDE_SUFFIXES {
            let mut side = db.as_os_str().to_owned();
            side.push(suffix);
            match tighten(Path::new(&side), 0o600) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/// Create `dir` (parents with default modes, `dir` itself 0700) and tighten
/// an existing app-owned `dir` to 0700.
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    return imp::create_private_dir(dir);
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Create the database file as 0600 if missing and tighten an existing
/// database and its `-wal`/`-shm`/`-journal` files to 0600. Call before
/// SQLite opens `db` ([`crate::Store::open`] does).
pub fn prepare_private_db(db: &Path) -> io::Result<()> {
    #[cfg(unix)]
    return imp::prepare_private_db(db);
    #[cfg(not(unix))]
    {
        let _ = db;
        Ok(())
    }
}
