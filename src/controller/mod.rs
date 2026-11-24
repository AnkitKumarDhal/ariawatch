mod aria2_process;
mod ipc;
mod launcher;
mod lock;
use crate::{config::persistence, logging, paths::AppPaths};
pub use lock::ControllerLock;
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[cfg(windows)]
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

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

pub fn ensure_running(paths: &AppPaths) -> Result<(), String> {
    launcher::ensure_running(paths)
}

pub fn stop(paths: &AppPaths) -> Result<(), String> {
    let response = ipc::send_command(paths, "STOP")?;

    if response != "STOPPING" {
        return Err(format!(
            "Unexpected response when stopping controller: {response}"
        ));
    }

    let pid_file = paths.controller_pid_file();
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        match fs::symlink_metadata(&pid_file) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                logging::info("Managed controller stopped successfully");
                return Ok(());
            }
            Ok(_) => {}
            Err(error) => {
                return Err(format!(
                    "Could not check controller shutdown status: {error}"
                ));
            }
        }

        if Instant::now() >= deadline {
            return Err(String::from(
                "Controller acknowledged shutdown but did not stop within 10 seconds",
            ));
        }

        thread::sleep(Duration::from_millis(50));
    }
}

// this one just loads the downloads directory from the config file
fn load_download_directory(paths: &AppPaths, home_dir: &Path) -> Result<PathBuf, String> {
    let config = persistence::load_or_create(&paths.config_file())?;
    Ok(config.downloads.resolve_directory(home_dir))
}

// just runs internal controller (testing)
pub fn run_internal() -> Result<(), String> {
    logging::init().map_err(|error| format!("Could not initialize controller logging: {error}"))?;
    logging::info("Controller bootstrap started");

    #[cfg(target_os = "linux")]
    {
        let result = unsafe { libc::daemon(1, 0) };

        if result != 0 {
            let error = io::Error::last_os_error();
            logging::error(&format!("Controller detachment failed: {error}"));
            return Err(format!("Could not detach controller: {error}"));
        }
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    {
        return Err(String::from(
            "Background controller startup currently supports only Linux and Windows",
        ));
    }

    let paths = AppPaths::discover()?;
    let _instance = ControllerInstance::acquire(&paths)?;

    #[cfg(unix)]
    let home_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| String::from("Could not determine the home directory"))?;

    #[cfg(windows)]
    let home_dir = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| String::from("Could not determine the home directory"))?;

    let download_dir = load_download_directory(&paths, &home_dir)?;

    let mut aria2 = aria2_process::ManagedAria2::start(&paths, &download_dir)?;

    ipc::run(&paths, &mut aria2)
}

impl Drop for ControllerInstance {
    fn drop(&mut self) {
        let is_regular_file = fs::symlink_metadata(&self.pid_file)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink());

        #[cfg(windows)]
        let is_regular_file = is_regular_file
            && fs::symlink_metadata(&self.pid_file).is_ok_and(|metadata| {
                metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
            });

        let is_our_pid_file = is_regular_file
            && fs::read_to_string(&self.pid_file)
                .is_ok_and(|contents| contents.trim() == self.pid.to_string());

        if is_our_pid_file {
            let _ = fs::remove_file(&self.pid_file);
        }
    }
}

fn write_pid_automatically(path: &Path, pid: u32) -> Result<(), String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let temp_path = path.with_file_name(format!(
        ".controller.pid.{}.{timestamp}.tmp",
        std::process::id()
    ));

    let result = (|| -> io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);

        #[cfg(unix)]
        options
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);

        #[cfg(windows)]
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);

        let mut file = options.open(&temp_path)?;

        #[cfg(windows)]
        {
            let metadata = file.metadata()?;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Temporary controller PID file is a reparse point",
                ));
            }
        }

        writeln!(file, "{pid}")?;
        file.sync_all()?;
        drop(file);

        #[cfg(windows)]
        remove_stale_pid_file(path)?;

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

#[cfg(windows)]
fn remove_stale_pid_file(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.is_file()
                && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 =>
        {
            fs::remove_file(path)
        }

        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Existing controller PID path is not a regular file",
        )),

        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),

        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{ControllerInstance, load_download_directory};
    use crate::paths::AppPaths;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn config_test_paths() -> (PathBuf, AppPaths, PathBuf) {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let root = std::env::temp_dir().join(format!(
            "ariawatch-controller-config-test-{}-{unique_id}",
            std::process::id()
        ));

        fs::create_dir(&root).unwrap();

        let home_dir = root.join("home");

        let paths = AppPaths {
            data_dir: root.join("data"),
            config_dir: root.join(".config/ariawatch"),
            state_dir: root.join("state"),
            runtime_dir: root.join("runtime"),
        };

        (root, paths, home_dir)
    }

    #[test]
    fn uses_default_download_directory_when_config_is_missing() {
        let (root, paths, home_dir) = config_test_paths();

        let download_dir = load_download_directory(&paths, &home_dir).unwrap();

        assert_eq!(download_dir, home_dir.join("Downloads"));
        assert!(paths.config_file().is_file());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_configured_relative_download_directory() {
        let (root, paths, home_dir) = config_test_paths();

        fs::create_dir_all(&paths.config_dir).unwrap();

        fs::write(
            paths.config_file(),
            "[downloads]\ndirectory = \"Documents/Downloads\"\n",
        )
        .unwrap();

        let download_dir = load_download_directory(&paths, &home_dir).unwrap();

        assert_eq!(download_dir, home_dir.join("Documents/Downloads"));

        fs::remove_dir_all(root).unwrap();
    }

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

        #[cfg(unix)]
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

    #[test]
    fn replaces_a_stale_pid_file() {
        let (root, paths) = test_paths();

        paths.prepare_runtime_dir().unwrap();

        let pid_file = paths.controller_pid_file();
        fs::write(&pid_file, "12345\n").unwrap();

        let instance = ControllerInstance::acquire(&paths).unwrap();

        assert_eq!(
            fs::read_to_string(&pid_file).unwrap().trim(),
            std::process::id().to_string()
        );

        drop(instance);

        assert!(!pid_file.exists());

        fs::remove_dir_all(root).unwrap();
    }
}
