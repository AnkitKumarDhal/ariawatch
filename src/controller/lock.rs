// this file, it holds the lock
use std::{
    fs::{File, OpenOptions, TryLockError},
    path::Path,
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

#[cfg(windows)]
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

#[derive(Debug)]
pub struct ControllerLock {
    _file: File,
}

impl ControllerLock {
    pub fn acquire(path: &Path) -> Result<Self, String> {
        let mut options = OpenOptions::new();

        options.read(true).write(true).create(true).truncate(false);

        #[cfg(unix)]
        options
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);

        #[cfg(windows)]
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);

        let file = options.open(path).map_err(|error| {
            format!(
                "COuld not open any controller lock file {}: {error}",
                path.display()
            )
        })?;

        let metadata = file.metadata().map_err(|error| {
            format!(
                "Could not inspect controller lock file {}: {error}",
                path.display()
            )
        })?;

        #[cfg(windows)]
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(format!(
                "Controller lock path must not be a reparse point: {}",
                path.display()
            ));
        }

        if !metadata.is_file() {
            return Err(format!(
                "Controller lock path is not a regular file: {}",
                path.display()
            ));
        }

        #[cfg(unix)]
        let current_uid = unsafe { libc::geteuid() };

        #[cfg(unix)]
        if metadata.uid() != current_uid {
            return Err(format!(
                "Controller lock file is not owned by the current user: {}",
                path.display()
            ));
        }

        #[cfg(unix)]
        if metadata.nlink() != 1 {
            return Err(format!(
                "Controller lock file must not have multiple hard links: {}",
                path.display()
            ));
        }

        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(String::from(
                    "Another ariawatch controller is already running",
                ));
            }
            Err(TryLockError::Error(error)) => {
                return Err(format!(
                    "Could not acqure controller lock {}: {error}",
                    path.display()
                ));
            }
        }

        #[cfg(unix)]
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

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }

        path
    }

    #[test]
    fn creates_controller_lock_file() {
        let directory = test_directory();
        let lock_path = directory.join("controller.lock");

        let lock = ControllerLock::acquire(&lock_path).unwrap();

        assert!(lock_path.is_file());

        drop(lock);
        fs::remove_dir_all(directory).unwrap();
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

        let third = ControllerLock::acquire(&lock_path).unwrap();

        drop(third);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symbolic_link_as_the_lock_file() {
        use std::os::unix::fs::symlink;

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

    #[cfg(unix)]
    #[test]
    fn rejects_a_lock_file_with_multiple_hard_links() {
        let directory = test_directory();
        let target = directory.join("target");
        let lock_path = directory.join("controller.lock");

        fs::write(&target, "lock file").unwrap();
        fs::hard_link(&target, &lock_path).unwrap();

        let result = ControllerLock::acquire(&lock_path);

        assert!(
            result
                .unwrap_err()
                .contains("must not have multiple hard links")
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn creates_lock_file_with_restrictive_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = test_directory();
        let lock_path = directory.join("controller.lock");

        let lock = ControllerLock::acquire(&lock_path).unwrap();

        let metadata = fs::metadata(&lock_path).unwrap();
        let permissions = metadata.permissions().mode() & 0o777;

        assert_eq!(permissions, 0o600);

        drop(lock);
        fs::remove_dir_all(directory).unwrap();
    }
}
