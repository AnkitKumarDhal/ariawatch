mod lock;

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub use lock::ControllerLock;

use crate::paths::AppPaths;

pub struct ControllerInstance {
    _lock: ControllerLock,
    pid_file: PathBuf,
    pid: u32,
}

impl ControllerInstance {
    pub fn acquire(paths: &AppPaths) -> Result<Self, String> {
        paths.prepare_runtime_dir()?;

        let lock = ControllerLock::acquire(&paths.controller_lock_file())?;
        let pid_file = paths.controller_pid_file();
        let pid = std::process::id();

        write_pid_automatically(&pid_file, pid)?;

        Ok(Self {
            _lock: lock,
            pid_file,
            pid,
        })
    }
}

impl Drop for ControllerInstance {
    fn drop(&mut self) {
        let is_our_pid_file = fs::symlink_metadata(&self.pid_file)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
            & fs::read_to_string(&self.pid_file)
                .is_ok_and(|contents| contents.trim() == self.pid.to_string());

        if is_our_pid_file {
            let _ = fs::remove_file(&self.pid_file);
        }
    }
}

fn write_pid_automatically(path: &PathBuf, pid: u32) -> Result<(), String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let temp_path = path.with_file_name(format!(
        ".controller.pid.{}.{timestamp}.tmp",
        std::process::id()
    ));

    let result = (|| -> io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(&temp_path)?;

        writeln!(file, "{pid}")?;
        file.sync_all()?;
        drop(file);

        fs::rename(&temp_path, path)?;

        Ok(())
    })();

    if let Err(error) = result {
        let _ = fs::remove_file(&temp_path);

        return Err(format!(
            "Could not write controller PID file {}: {error}",
            path.display()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ControllerInstance;
    use crate::paths::AppPaths;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn test_paths() -> (PathBuf, AppPaths) {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let root = std::env::temp_dir().join(format!(
            "ariawatch-instance-test-{}-{unique_id}",
            std::process::id()
        ));

        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();

        let paths = AppPaths {
            data_dir: root.join("data"),
            config_dir: root.join("config"),
            state_dir: root.join("state"),
            runtime_dir: root.join("runtime"),
        };

        (root, paths)
    }

    #[test]
    fn manages_pid_file_and_exclusive_lock_lifecycle() {
        let (root, paths) = test_paths();
        let pid_file = paths.controller_pid_file();

        let first = ControllerInstance::acquire(&paths).unwrap();

        assert_eq!(
            fs::read_to_string(&pid_file).unwrap().trim(),
            std::process::id().to_string()
        );

        // A second instance cannot acquire the lock while the first is alive.
        assert!(ControllerInstance::acquire(&paths).is_err());
        assert!(pid_file.exists());

        drop(first);

        // Dropping the instance removes its PID file and releases its lock.
        assert!(!pid_file.exists());

        let second = ControllerInstance::acquire(&paths).unwrap();
        assert!(pid_file.exists());

        drop(second);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn does_not_delete_a_pid_file_replaced_by_another_value() {
        let (root, paths) = test_paths();
        let pid_file = paths.controller_pid_file();

        let instance = ControllerInstance::acquire(&paths).unwrap();

        fs::write(&pid_file, "999999\n").unwrap();

        drop(instance);

        assert_eq!(fs::read_to_string(&pid_file).unwrap(), "999999\n");

        fs::remove_dir_all(root).unwrap();
    }
}
