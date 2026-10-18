use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

static LOG_FILE: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

pub fn init() -> Result<PathBuf, String> {
    let state_dir = std::env::var("XDG_STATE_HOME")
        .ok()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("state"))
        })
        .ok_or_else(|| String::from("Could not determine state directory"))?;

    let log_dir = state_dir.join("ariawatch");

    fs::create_dir_all(&log_dir)
        .map_err(|error| format!("Could not create log directory: {error}"))?;

    let log_path = log_dir.join("ariawatch.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|error| format!("Could not open log file: {error}"))?;

    LOG_FILE
        .set(Mutex::new(file))
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
    let Some(file) = LOG_FILE.get() else {
        return;
    };

    let Ok(mut file) = file.lock() else {
        return;
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let _ = writeln!(file, "[{timestamp}] {level}: {message}");
    let _ = file.flush();
}
