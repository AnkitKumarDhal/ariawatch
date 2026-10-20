// Just simply (not so simple) writes logs to a file in .local/state/ariawatch.log
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
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

pub fn init() -> Result<PathBuf, String> {
    let paths = AppPaths::discover()?;

    fs::create_dir_all(&paths.state_dir)
        .map_err(|error| format!("Could not create log directory: {error}"))?;

    let log_path = paths.state_dir.join("ariawatch.log");

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| format!("Could not open log file: {error}"))?;

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

    if !is_full {
        return false;
    }

    if log.file.flush().is_err() {
        return false;
    }

    let backup_path = log.path.with_extension("log.1");

    if fs::copy(&log.path, &backup_path).is_err() {
        return false;
    }

    if log.file.set_len(0).is_err() {
        return false;
    }

    true
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
}
