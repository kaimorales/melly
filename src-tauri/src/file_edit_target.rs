use anyhow::Result;

#[cfg(target_os = "macos")]
pub use macos::Target;

#[cfg(target_os = "macos")]
mod macos {
    use super::Result;
    use anyhow::{Context, ensure};
    use melly_core::file_edits::MAX_FILE_BYTES;
    use std::{
        ffi::{CStr, CString},
        fs::{File, Metadata, OpenOptions},
        io::{self, Read, Write},
        os::{
            fd::{AsRawFd, FromRawFd, IntoRawFd},
            unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
        },
        path::{Component, Path, PathBuf},
        sync::Arc,
    };

    #[derive(Clone)]
    pub struct Target(Arc<Selection>);

    struct Selection {
        parent: File,
        root: PathBuf,
        identity: (u64, u64),
        relative: PathBuf,
        name: CString,
        original: Option<Snapshot>,
    }

    struct Snapshot {
        bytes: String,
        metadata: Metadata,
    }

    impl Target {
        pub fn select(
            root: &Path,
            identity: (u64, u64),
            relative: &Path,
            allow_create: bool,
        ) -> Result<Self> {
            let name = relative
                .file_name()
                .and_then(|name| name.to_str())
                .context("The proposal must name a project file")?;
            let name = CString::new(name)?;
            let parent = scoped_parent(root, identity, relative)?;
            let original = if allow_create && absent(&parent, &name) {
                None
            } else {
                Some(read_regular(&parent, &name)?.1)
            };
            Ok(Self(Arc::new(Selection {
                parent,
                root: root.to_owned(),
                identity,
                relative: relative.to_owned(),
                name,
                original,
            })))
        }

        pub fn original(&self) -> Option<&str> {
            self.0
                .original
                .as_ref()
                .map(|snapshot| snapshot.bytes.as_str())
        }

        pub fn original_mode(&self) -> Option<u32> {
            self.0
                .original
                .as_ref()
                .map(|snapshot| snapshot.metadata.mode() & 0o777)
        }

        pub fn delete(&self) -> Result<()> {
            let selection = &self.0;
            let original = selection.original.as_ref().context("The file is missing")?;
            selection.verify_directory()?;
            selection.verify(original)?;
            let removed =
                unsafe { libc::unlinkat(selection.parent.as_raw_fd(), selection.name.as_ptr(), 0) };
            check(removed).context("Could not delete the selected file")
        }

        pub fn entries(
            root: &Path,
            identity: (u64, u64),
            relative: &Path,
            limit: usize,
        ) -> Result<Vec<(String, bool)>> {
            let parent = scoped_parent(root, identity, &relative.join("entry"))?;
            let fd = parent.into_raw_fd();
            let directory = unsafe { libc::fdopendir(fd) };
            if directory.is_null() {
                unsafe {
                    libc::close(fd);
                }
                anyhow::bail!("Could not open project directory");
            }
            let mut entries = Vec::new();
            for _ in 0..limit {
                let entry = unsafe { libc::readdir(directory) };
                if entry.is_null() {
                    break;
                }
                let entry = unsafe { &*entry };
                if !matches!(entry.d_type, libc::DT_DIR | libc::DT_REG) {
                    continue;
                }
                let name = unsafe { CStr::from_ptr(entry.d_name.as_ptr()) };
                if let Ok(name) = name.to_str()
                    && !crate::linked_projects::excluded(name)
                {
                    entries.push((name.to_owned(), entry.d_type == libc::DT_DIR));
                }
            }
            let more = !unsafe { libc::readdir(directory) }.is_null();
            unsafe {
                libc::closedir(directory);
            }
            ensure!(
                !more,
                "Too many directory entries. Search a narrower project subdirectory."
            );
            entries.sort_unstable();
            Ok(entries)
        }

        pub fn write(&self, updated: &str) -> Result<()> {
            self.write_restoring_mode(updated, None)
        }

        pub fn write_restoring_mode(&self, updated: &str, mode: Option<u32>) -> Result<()> {
            let selection = &self.0;
            ensure!(
                updated.len() <= MAX_FILE_BYTES && !updated.contains('\0'),
                "Invalid file edit contents"
            );
            let parent = &selection.parent;
            selection.verify_directory()?;
            let existing = selection
                .original
                .as_ref()
                .map(|original| selection.verify(original))
                .transpose()?;
            let name = CString::new(format!(".melly-edit-{}", uuid::Uuid::new_v4()))?;
            // All mutations use the verified linked directory descriptor.
            let mut file = open_at(parent, &name, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)
                .context("Could not stage the file in the selected folder")?;
            let staged = Staged { parent, name };
            let result = (|| {
                file.write_all(updated.as_bytes())
                    .context("Could not write the edit")?;
                if let Some((original, metadata)) = existing {
                    // Retain ACLs, extended attributes, group, and permissions on the replacement.
                    let copied = unsafe {
                        libc::fcopyfile(
                            original.as_raw_fd(),
                            file.as_raw_fd(),
                            std::ptr::null_mut(),
                            libc::COPYFILE_ACL | libc::COPYFILE_XATTR,
                        )
                    };
                    check(copied).context("Could not preserve the file metadata")?;
                    if file.metadata()?.gid() != metadata.gid() {
                        let changed = unsafe { libc::fchown(file.as_raw_fd(), !0, metadata.gid()) };
                        check(changed).context("Could not preserve the file group")?;
                    }
                    file.set_permissions(std::fs::Permissions::from_mode(metadata.mode() & 0o777))?;
                }
                if let Some(mode) = mode {
                    file.set_permissions(std::fs::Permissions::from_mode(mode & 0o777))?;
                }
                file.sync_all().context("Could not save the staged edit")?;
                selection.verify_directory()?;
                if let Some(original) = &selection.original {
                    selection.verify(original)?;
                }
                // Exclusive rename makes new-file creation fail if the name has
                // been taken since the save panel closed.
                let flags = if selection.original.is_some() {
                    0
                } else {
                    libc::RENAME_EXCL
                };
                let renamed = unsafe {
                    libc::renameatx_np(
                        parent.as_raw_fd(),
                        staged.name.as_ptr(),
                        parent.as_raw_fd(),
                        selection.name.as_ptr(),
                        flags,
                    )
                };
                check(renamed)
                    .context("Could not replace the selected file or the name is already in use")?;
                Ok(())
            })();
            drop(file);
            drop(staged);
            result
        }
    }

    impl Selection {
        fn verify_directory(&self) -> Result<()> {
            let current = scoped_parent(&self.root, self.identity, &self.relative)?;
            ensure!(
                same_inode(&current.metadata()?, &self.parent.metadata()?),
                "The project directory changed. Ask for a new edit."
            );
            Ok(())
        }

        fn verify(&self, expected: &Snapshot) -> Result<(File, Metadata)> {
            let (file, actual) = read_regular(&self.parent, &self.name)
                .context("The selected file changed or is unavailable. Choose it again.")?;
            ensure!(
                same_version(&expected.metadata, &actual.metadata)
                    && expected.bytes == actual.bytes,
                "The file changed after review. Review the proposal again."
            );
            Ok((file, actual.metadata))
        }
    }

    struct Staged<'a> {
        parent: &'a File,
        name: CString,
    }

    impl Drop for Staged<'_> {
        fn drop(&mut self) {
            unsafe { libc::unlinkat(self.parent.as_raw_fd(), self.name.as_ptr(), 0) };
        }
    }

    fn open_directory(path: &Path) -> Result<File> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(path)
            .context("The selected directory is unavailable")
    }

    fn open_at(parent: &File, name: &CStr, flags: libc::c_int) -> io::Result<File> {
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
                0o600 as libc::c_int,
            )
        };
        check(fd)?;
        // A successful open provides an owned file descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }

    fn read_regular(parent: &File, name: &CStr) -> Result<(File, Snapshot)> {
        let file = open_at(parent, name, libc::O_RDWR)
            .context("Could not open the selected file for editing")?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.len() <= MAX_FILE_BYTES as u64,
            "Choose a UTF-8 text file of 24 KiB or less"
        );
        ensure!(
            metadata.uid() == unsafe { libc::geteuid() }
                && metadata.nlink() == 1
                && metadata.mode() & 0o7000 == 0,
            "Choose a regular file you own without additional hard links or special permissions"
        );
        let mut bytes = String::new();
        (&file)
            .take(MAX_FILE_BYTES as u64 + 1)
            .read_to_string(&mut bytes)
            .context("Choose a valid UTF-8 text file")?;
        ensure!(
            bytes.len() <= MAX_FILE_BYTES && !bytes.contains('\0'),
            "Choose a UTF-8 text file of 24 KiB or less"
        );
        ensure!(
            same_version(&metadata, &file.metadata()?),
            "The file changed while it was being read. Choose it again."
        );
        Ok((file, Snapshot { bytes, metadata }))
    }

    fn absent(parent: &File, name: &CStr) -> bool {
        open_at(parent, name, libc::O_RDONLY)
            .is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
    }

    fn scoped_parent(root: &Path, identity: (u64, u64), relative: &Path) -> Result<File> {
        let inside = relative
            .parent()
            .context("The proposal must name a project file")?;
        let mut directory = open_directory(root)?;
        let metadata = directory.metadata()?;
        ensure!(
            (metadata.dev(), metadata.ino()) == identity,
            "The linked folder moved. Reconnect it."
        );
        for component in relative.components() {
            let Component::Normal(name) = component else {
                anyhow::bail!("The proposed file must be inside the linked project");
            };
            let name = name
                .to_str()
                .context("The proposed file must have a UTF-8 path")?;
            ensure!(
                !crate::linked_projects::excluded(name),
                "The proposal targets an excluded project path"
            );
        }
        for part in inside.iter() {
            let component = CString::new(part.to_str().context("Invalid project directory name")?)?;
            directory = open_at(&directory, &component, libc::O_RDONLY | libc::O_DIRECTORY)
                .context("The proposed directory is missing or is a symbolic link")?;
        }
        Ok(directory)
    }

    fn same_inode(a: &Metadata, b: &Metadata) -> bool {
        a.dev() == b.dev() && a.ino() == b.ino()
    }

    fn same_version(a: &Metadata, b: &Metadata) -> bool {
        same_inode(a, b)
            && a.len() == b.len()
            && a.mtime() == b.mtime()
            && a.mtime_nsec() == b.mtime_nsec()
            && a.ctime() == b.ctime()
            && a.ctime_nsec() == b.ctime_nsec()
    }

    fn check(result: libc::c_int) -> io::Result<()> {
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn scratch() -> (PathBuf, impl Drop) {
            struct Scratch(PathBuf);
            impl Drop for Scratch {
                fn drop(&mut self) {
                    let _ = std::fs::remove_dir_all(&self.0);
                }
            }
            let path =
                std::env::temp_dir().join(format!("melly-file-edit-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&path).unwrap();
            (path.clone(), Scratch(path))
        }

        fn select(root: &Path, path: &Path, create: bool) -> Result<Target> {
            let identity = melly_core::linked::root_identity(root)?;
            Target::select(root, identity, path.strip_prefix(root)?, create)
        }

        #[test]
        fn applies_only_the_selected_file_and_rejects_stale_proposals() {
            let (folder, _cleanup) = scratch();
            let path = folder.join("untitled.txt");
            std::fs::write(&path, "before").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o751)).unwrap();
            let target = select(&folder, &path, false).unwrap();
            assert_eq!(target.original(), Some("before"));
            target.write("after").unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "after");
            assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o751);
            assert!(target.write("stale").is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "after");
            assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        }

        #[test]
        fn creates_exclusively_and_refuses_links_or_a_moved_directory() {
            let (folder, _cleanup) = scratch();
            let path = folder.join("new.txt");
            let target = select(&folder, &path, true).unwrap();
            std::fs::write(&path, "other process").unwrap();
            assert!(target.write("ours").is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "other process");
            let link = folder.join("link");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(select(&folder, &link, false).is_err());
            assert!(select(&folder, &link, true).is_err());

            let sub = folder.join("sub");
            std::fs::create_dir(&sub).unwrap();
            let target = select(&folder, &sub.join("new.txt"), true).unwrap();
            std::fs::rename(&sub, folder.join("moved")).unwrap();
            std::fs::create_dir(&sub).unwrap();
            assert!(target.write("ours").is_err());
            assert!(!sub.join("new.txt").exists());
            let created = sub.join("created.txt");
            select(&folder, &created, true)
                .unwrap()
                .write("new text")
                .unwrap();
            assert_eq!(std::fs::read_to_string(created).unwrap(), "new text");
        }
    }
}

#[cfg(not(target_os = "macos"))]
#[derive(Clone)]
pub struct Target;

#[cfg(not(target_os = "macos"))]
impl Target {
    pub fn select(
        _: &std::path::Path,
        _: (u64, u64),
        _: &std::path::Path,
        _: bool,
    ) -> Result<Self> {
        anyhow::bail!("File editing requires macOS")
    }
    pub fn delete(&self) -> Result<()> {
        anyhow::bail!("File editing requires macOS")
    }
    pub fn entries(
        _: &std::path::Path,
        _: (u64, u64),
        _: &std::path::Path,
        _: usize,
    ) -> Result<Vec<(String, bool)>> {
        anyhow::bail!("Project search requires macOS")
    }
    pub fn write(&self, _: &str) -> Result<()> {
        anyhow::bail!("File editing requires macOS")
    }
    pub fn original(&self) -> Option<&str> {
        unreachable!()
    }
    pub fn original_mode(&self) -> Option<u32> {
        unreachable!()
    }
    pub fn write_restoring_mode(&self, _: &str, _: Option<u32>) -> Result<()> {
        anyhow::bail!("File editing requires macOS")
    }
}
