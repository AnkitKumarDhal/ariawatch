use std::{
    fs::{File, OpenOptions},
    io,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::Path,
};

// this file, it holds the lock
#[derive(Debug)]
pub struct ControllerLock {
    _file: File,
}

impl ControllerLock {
    pub fn acquire(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(path)
            .map_err(|error| {
                format!(
                    "Could not open any controller lock file {}: {error}",
                    path.display()
                )
            })?;

        let metadata = file.metadata().map_err(|error| {
            format!(
                "Could not inspect controller lock file {}: {error}",
                path.display()
            )
        })?;

        if !metadata.is_file() {
            return Err(format!(
                "Controller lock path is not a regular file: {}",
                path.display()
            ));
        }

        let current_uid = unsafe { libc::geteuid() };

        if metadata.uid() != current_uid {
            return Err(format!(
                "Controller lock file is not owned by the current user: {}",
                path.display()
            ));
        }

        if metadata.nlink() != 1 {
            return Err(format!(
                "Controller lock file must not have multiple hard links: {}",
                path.display()
            ));
        }

        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };

        if result != 0 {
            let error = io::Error::last_os_error();

            if error.kind() == io::ErrorKind::WouldBlock {
                return Err(String::from(
                    "Another ariawatch controller is already running",
                ));
            }

            return Err(format!(
                "Could not acquire controller lock {}: {error}",
                path.display()
            ));
        }

        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|error| {
                format!(
                    "Could not secure controller lock file {}: {error}",
                    path.display()
                )
            })?;

        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::ControllerLock;
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn test_directory() -> PathBuf {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let path = std::env::temp_dir().join(format!(
            "ariawatch-lock-test-{}-{unique_id}",
            std::process::id()
        ));

        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();

        path
    }

    #[test]
    fn prevents_a_second_lock_until_the_first_is_dropped() {
        let directory = test_directory();
        let lock_path = directory.join("controller.lock");

        let first = ControllerLock::acquire(&lock_path).unwrap();

        let second = ControllerLock::acquire(&lock_path);
        assert!(
            second
                .unwrap_err()
                .contains("Another ariawatch controller is already running")
        );

        drop(first);

        // Once the first lock is released, another controller can acquire it.
        let third = ControllerLock::acquire(&lock_path).unwrap();
        drop(third);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_a_symbolic_link_as_the_lock_file() {
        let directory = test_directory();
        let target = directory.join("target");
        let lock_path = directory.join("controller.lock");

        fs::write(&target, "leave this file unchanged").unwrap();
        symlink(&target, &lock_path).unwrap();

        assert!(ControllerLock::acquire(&lock_path).is_err());
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "leave this file unchanged"
        );

        fs::remove_dir_all(directory).unwrap();
    }
}
