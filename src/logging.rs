// Just simply (not so simple) writes logs to a file in .local/state/ariawatch.log
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::paths::AppPaths;

const MAX_LOG_SIZE: u64 = 1024 * 1024;
struct LogFile {
    file: std::fs::File,
    path: PathBuf,
}

static LOG_FILE: OnceLock<Mutex<LogFile>> = OnceLock::new();

fn open_log_file(path: &Path) -> Result<std::fs::File, String> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| format!("Could not open log file {}: {error}", path.display()))?;

    let metadata = file
        .metadata()
        .map_err(|error| format!("Could not inspect log file: {error}"))?;

    if !metadata.is_file() {
        return Err(format!(
            "Log path is not a regular file: {}",
            path.display()
        ));
    }

    let current_uid = unsafe { libc::geteuid() };

    if metadata.uid() != current_uid {
        return Err(format!(
            "Log file is not owned by current user: {}",
            path.display()
        ));
    }

    if metadata.nlink() != 1 {
        return Err(format!(
            "Log file must not have multiple hard links: {}",
            path.display()
        ));
    }

    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("Could not secure log file: {error}"))?;

    Ok(file)
}

pub fn init() -> Result<PathBuf, String> {
    let paths = AppPaths::discover()?;
    fs::create_dir_all(&paths.state_dir)
        .map_err(|error| format!("Could not create log directory: {error}"))?;
    let log_path = paths.state_dir.join("ariawatch.log");
    let file = open_log_file(&log_path)?;

    LOG_FILE
        .set(Mutex::new(LogFile {
            file,
            path: log_path.clone(),
        }))
        .map_err(|_| String::from("Logger is already initialized"))?;

    Ok(log_path)
}

pub fn info(message: &str) {
    write("INFO", message);
}

pub fn warn(message: &str) {
    write("WARN", message);
}

pub fn error(message: &str) {
    write("ERROR", message);
}

fn write(level: &str, message: &str) {
    let Some(log) = LOG_FILE.get() else {
        return;
    };

    let Ok(mut log) = log.lock() else {
        return;
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if rotate_if_needed(&mut log) {
        let backup_path = log.path.with_extension("log.1");
        let _ = writeln!(
            log.file,
            "[{timestamp}] WARN: Previous log saved to {}",
            backup_path.display()
        );
    }

    let _ = writeln!(log.file, "[{timestamp}] {level}: {message}");
    let _ = log.file.flush();
}

fn rotate_if_needed(log: &mut LogFile) -> bool {
    let is_full = log
        .file
        .metadata()
        .is_ok_and(|metadata| metadata.len() >= MAX_LOG_SIZE);

    if !is_full || log.file.flush().is_err() {
        return false;
    }

    let backup_path = log.path.with_extension("log.1");

    if log
        .file
        .set_permissions(fs::Permissions::from_mode(0o600))
        .is_err()
    {
        return false;
    }

    if fs::rename(&log.path, &backup_path).is_err() {
        return false;
    }

    match open_log_file(&log.path) {
        Ok(file) => {
            log.file = file;
            true
        }
        Err(_) => {
            let _ = fs::rename(&backup_path, &log.path);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LogFile, MAX_LOG_SIZE, rotate_if_needed};
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        time::{SystemTime, UNIX_EPOCH},
    };

    // Tests rotation when the log exceeds its size limit.
    #[test]
    fn rotates_full_log() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let directory = std::env::temp_dir().join(format!("ariawatch-log-test-{unique_id}"));
        fs::create_dir_all(&directory).unwrap();

        let path = directory.join("ariawatch.log");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();

        file.write_all(&vec![b'x'; (MAX_LOG_SIZE + 1) as usize])
            .unwrap();

        let mut log = LogFile {
            file,
            path: path.clone(),
        };

        assert!(rotate_if_needed(&mut log));
        assert_eq!(log.file.metadata().unwrap().len(), 0);

        let backup_path = path.with_extension("log.1");
        assert_eq!(fs::metadata(backup_path).unwrap().len(), MAX_LOG_SIZE + 1);

        drop(log);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn creates_log_file_with_private_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = std::env::temp_dir().join(format!(
            "ariawatch-log-permissions-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        fs::create_dir_all(&directory).unwrap();

        let path = directory.join("ariawatch.log");
        let file = super::open_log_file(&path).unwrap();

        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);

        drop(file);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symbolic_link_as_log_file() {
        use std::os::unix::fs::symlink;

        let directory = std::env::temp_dir().join(format!(
            "ariawatch-log-symlink-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        fs::create_dir_all(&directory).unwrap();

        let target = directory.join("target");
        let log_path = directory.join("ariawatch.log");

        fs::write(&target, "preserve this content").unwrap();
        symlink(&target, &log_path).unwrap();

        assert!(super::open_log_file(&log_path).is_err());
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "preserve this content"
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rotation_replaces_backup_symlink_without_overwriting_target() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let directory = std::env::temp_dir().join(format!(
            "ariawatch-log-rotation-symlink-{}-{unique_id}",
            std::process::id()
        ));

        fs::create_dir_all(&directory).unwrap();

        let path = directory.join("ariawatch.log");
        let backup_path = path.with_extension("log.1");
        let target = directory.join("important.txt");

        fs::write(&target, "keep this file unchanged").unwrap();
        symlink(&target, &backup_path).unwrap();

        let mut file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(&path)
            .unwrap();

        file.write_all(&vec![b'x'; (MAX_LOG_SIZE + 1) as usize])
            .unwrap();

        let mut log = LogFile {
            file,
            path: path.clone(),
        };

        assert!(rotate_if_needed(&mut log));

        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "keep this file unchanged"
        );

        let backup_metadata = fs::symlink_metadata(&backup_path).unwrap();
        assert!(!backup_metadata.file_type().is_symlink());
        assert_eq!(backup_metadata.permissions().mode() & 0o777, 0o600);
        assert_eq!(log.file.metadata().unwrap().len(), 0);

        drop(log);
        fs::remove_dir_all(directory).unwrap();
    }
}
