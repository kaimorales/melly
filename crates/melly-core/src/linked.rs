//! Descriptor-relative reads reject symlinks at every component.
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Component, Path},
};

pub fn revision(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(unix)]
pub fn root_identity(root: &Path) -> Result<(u64, u64)> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)
        .context("Linked folder is unavailable. Choose its new location to reconnect it.")?;
    let meta = file.metadata()?;
    Ok((meta.dev(), meta.ino()))
}

#[cfg(unix)]
pub fn read_scoped(root: &Path, identity: Option<(u64, u64)>, relative: &Path) -> Result<Vec<u8>> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{
                ffi::OsStrExt,
                fs::{MetadataExt, OpenOptionsExt},
            },
        },
    };
    let mut directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)
        .context("Project folder is unavailable. Refresh or reconnect it.")?;
    if let Some((device, inode)) = identity {
        let meta = directory.metadata()?;
        ensure!(
            meta.dev() == device && meta.ino() == inode,
            "Linked folder moved or was replaced. Reconnect it."
        );
    }
    let components: Vec<_> = relative.components().collect();
    ensure!(!components.is_empty(), "A file is required");
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            anyhow::bail!("File must be inside its project folder");
        };
        let name = CString::new(name.as_bytes())?;
        let final_component = index + 1 == components.len();
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC
            | libc::O_NONBLOCK
            | if final_component {
                0
            } else {
                libc::O_DIRECTORY
            };
        // The open parent descriptor remains alive until its child has opened.
        let descriptor = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        ensure!(
            descriptor >= 0,
            "File moved, became a link, or is inaccessible. Refresh project files."
        );
        directory = unsafe { File::from_raw_fd(descriptor) };
    }
    let metadata = directory.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= crate::projects::MAX_FILE_BYTES,
        "File exceeds the supported size or is not a regular file"
    );
    let mut bytes = Vec::new();
    directory
        .take(crate::projects::MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= crate::projects::MAX_FILE_BYTES,
        "File exceeds the supported size"
    );
    Ok(bytes)
}
#[cfg(not(unix))]
pub fn root_identity(_: &Path) -> Result<(u64, u64)> {
    anyhow::bail!("Linked folders require a Unix host")
}
#[cfg(not(unix))]
pub fn read_scoped(_: &Path, _: Option<(u64, u64)>, _: &Path) -> Result<Vec<u8>> {
    anyhow::bail!("Scoped file reads require a Unix host")
}
