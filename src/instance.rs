use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::paths;

pub struct InstanceLock {
    _file: std::fs::File,
}

pub fn acquire_instance_lock() -> Result<InstanceLock> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(paths::cache_dir);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("cannot create runtime directory {}", dir.display()))?;
    let path = dir.join("thepaper.lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("cannot open instance lock {}", path.display()))?;
    use std::os::fd::AsRawFd;
    let r = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if r != 0 {
        anyhow::bail!(
            "another thepaper instance is already running (lock {})",
            path.display()
        );
    }
    Ok(InstanceLock { _file: file })
}
