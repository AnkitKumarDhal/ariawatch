use crate::{
    aria2,
    config::{DEFAULT_RPC_URL, ExternalServerConfig, RuntimeConfig, RuntimeMode},
    controller,
    domain::{download::Download, status::Status},
    logging,
    monitor::{self, MonitorUpdate},
    paths::AppPaths,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc::Receiver,
};

pub struct DownloadSnapshot {
    pub active_count: usize,
    pub waiting_count: usize,
    pub stopped_count: usize,
    pub downloads: Vec<Download>,
    pub is_complete: bool,
}

#[derive(Debug, Default)]
pub struct FileDeleteReport {
    pub deleted_files: usize,
    pub failed_files: usize,
}

// converts each response individually,,, even one conversion failure marks the whole snapshot as invalid
fn convert_snapshot_responses(
    responses: Vec<aria2::Aria2DownloadResponse>,
) -> (Vec<Download>, bool) {
    let mut downloads = Vec::with_capacity(responses.len());
    let mut is_complete = true;

    for response in responses {
        let gid = response.gid.clone();

        match response.into_download() {
            Ok(download) => downloads.push(download),
            Err(error) => {
                is_complete = false;
                logging::warn(&format!(
                    "Coud not parse downlaod {gid} from aria2 snapshot; skipping it and disabling history reconcilation: {error}"
                ));
            }
        }
    }

    (downloads, is_complete)
}

pub trait DownloadBackend {
    fn get_version(&self) -> Result<String, String>;
    fn fetch_snapshot(&self) -> Result<DownloadSnapshot, String>;
    fn start_monitor(
        &self,
        initial_active_gids: Vec<String>,
    ) -> Result<Receiver<MonitorUpdate>, String>;

    fn add_uri(&self, uri: &str) -> Result<String, String>;
    fn add_torrent_file(&self, path: &Path) -> Result<String, String>;

    fn pause_download(&self, gid: &str) -> Result<(), String>;
    fn resume_download(&self, gid: &str) -> Result<(), String>;
    fn pause_all(&self) -> Result<(), String>;
    fn resume_all(&self) -> Result<(), String>;

    fn remove_download(&self, gid: &str, status: &Status) -> Result<(), String>;
    fn remove_download_and_delete_files(
        &self,
        gid: &str,
        status: &Status,
    ) -> Result<FileDeleteReport, String>;
}

struct ExternalAria2Backend {
    config: ExternalServerConfig,
    managed_mode: bool,
}

impl ExternalAria2Backend {
    fn new(config: ExternalServerConfig, managed_mode: bool) -> Self {
        Self {
            config,
            managed_mode,
        }
    }
}

// managed controller starts aria2 with $HOME/Downloads as its directory
fn managed_download_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| String::from("Could not determine the home directory"))?;

    let directory = home.join("Downloads");

    let canonical = fs::canonicalize(&directory).map_err(|error| {
        format!(
            "Could not resolve managed download directory {}: {error}",
            directory.display()
        )
    })?;

    if !canonical.is_dir() {
        return Err(String::from(
            "Managed downlaod directory is not a directory",
        ));
    }

    Ok(canonical)
}

// validate the files before deleting them
fn collect_safe_files(paths: Vec<PathBuf>, directory: &Path) -> Result<Vec<PathBuf>, String> {
    let canonical_directory = fs::canonicalize(directory).map_err(|error| {
        format!(
            "Could not resolve managed download directory {}: {error}",
            directory.display()
        )
    })?;

    let mut safe_files = Vec::new();

    for path in paths {
        let candidate = if path.is_absolute() {
            path
        } else {
            canonical_directory.join(path)
        };

        let metadata = match fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "Could not inspect download file {}: {error}",
                    candidate.display()
                ));
            }
        };

        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Refusing to delete a symbolic link: {}",
                candidate.display()
            ));
        }

        if !metadata.is_file() {
            return Err(format!(
                "Refusing to delete a non-regular file: {}",
                candidate.display()
            ));
        }

        let canonical_file = fs::canonicalize(&candidate).map_err(|error| {
            format!(
                "Could not resolve download file {}: {error}",
                candidate.display()
            )
        })?;

        if canonical_file == canonical_directory
            || !canonical_file.starts_with(&canonical_directory)
        {
            return Err(format!(
                "Refusing to delete a file outside the managed download directory: {}",
                canonical_file.display()
            ));
        }

        safe_files.push(canonical_file);
    }

    safe_files.sort();
    safe_files.dedup();

    Ok(safe_files)
}

impl DownloadBackend for ExternalAria2Backend {
    fn get_version(&self) -> Result<String, String> {
        let request = aria2::build_get_version_request(self.config.rpc_secret.as_deref())?;
        let response = aria2::send_rpc_request(&self.config.rpc_url, &request)?;
        let rpc_response = aria2::parse_rpc_response(&response)?;
        let result = rpc_response.into_result("ariawatch-1")?;

        Ok(result
            .get("version")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .to_owned())
    }

    fn fetch_snapshot(&self) -> Result<DownloadSnapshot, String> {
        let (active_count, waiting_count, stopped_count, responses) =
            aria2::fetch_all_downloads(&self.config.rpc_url, self.config.rpc_secret.as_deref())?;

        let (downloads, is_complete) = convert_snapshot_responses(responses);

        Ok(DownloadSnapshot {
            active_count,
            waiting_count,
            stopped_count,
            downloads,
            is_complete,
        })
    }

    fn start_monitor(
        &self,
        initial_active_gids: Vec<String>,
    ) -> Result<Receiver<MonitorUpdate>, String> {
        monitor::start(
            self.config.rpc_url.clone(),
            self.config.rpc_secret.clone(),
            initial_active_gids,
        )
    }

    fn add_uri(&self, uri: &str) -> Result<String, String> {
        aria2::add_uri(&self.config.rpc_url, uri, self.config.rpc_secret.as_deref())
    }

    fn add_torrent_file(&self, path: &Path) -> Result<String, String> {
        aria2::add_torrent_file(
            &self.config.rpc_url,
            path,
            self.config.rpc_secret.as_deref(),
        )
    }

    fn pause_download(&self, gid: &str) -> Result<(), String> {
        aria2::pause_download(&self.config.rpc_url, gid, self.config.rpc_secret.as_deref())
    }

    fn resume_download(&self, gid: &str) -> Result<(), String> {
        aria2::resume_download(&self.config.rpc_url, gid, self.config.rpc_secret.as_deref())
    }

    fn pause_all(&self) -> Result<(), String> {
        aria2::pause_all(&self.config.rpc_url, self.config.rpc_secret.as_deref())
    }

    fn resume_all(&self) -> Result<(), String> {
        aria2::resume_all(&self.config.rpc_url, self.config.rpc_secret.as_deref())
    }

    fn remove_download(&self, gid: &str, status: &Status) -> Result<(), String> {
        aria2::remove_download(
            &self.config.rpc_url,
            gid,
            status,
            self.config.rpc_secret.as_deref(),
        )
    }

    fn remove_download_and_delete_files(
        &self,
        gid: &str,
        status: &Status,
    ) -> Result<FileDeleteReport, String> {
        if !self.managed_mode {
            return Err(String::from(
                "Deleting downloaded files is only available in managed mode",
            ));
        }

        let directory = managed_download_dir()?;
        let file_paths = aria2::get_download_file_paths(
            &self.config.rpc_url,
            gid,
            self.config.rpc_secret.as_deref(),
        )?;
        let safe_files = collect_safe_files(file_paths, &directory)?;

        aria2::remove_download(
            &self.config.rpc_url,
            gid,
            status,
            self.config.rpc_secret.as_deref(),
        )?;

        let mut report = FileDeleteReport::default();

        for path in safe_files {
            match fs::remove_file(&path) {
                Ok(()) => report.deleted_files += 1,
                Err(error) => {
                    report.failed_files += 1;
                    logging::error(&format!(
                        "Download was removed from aria2, but file deletion failed for {}: {error}",
                        path.display()
                    ));
                }
            }
        }

        Ok(report)
    }
}

pub fn create_backend(config: RuntimeConfig) -> Result<Box<dyn DownloadBackend>, String> {
    match config.mode {
        RuntimeMode::Managed => {
            let paths = AppPaths::discover()?;
            controller::ensure_running(&paths)?;

            let rpc_secret = read_managed_rpc_secret(&paths.rpc_secret_file())?;
            let server = ExternalServerConfig {
                rpc_url: DEFAULT_RPC_URL.to_owned(),
                rpc_secret: Some(rpc_secret),
            };

            Ok(Box::new(ExternalAria2Backend::new(server, true)))
        }

        RuntimeMode::External(external) => Ok(Box::new(ExternalAria2Backend::new(external, false))),
    }
}

fn read_managed_rpc_secret(path: &Path) -> Result<String, String> {
    let contents = fs::read_to_string(path).map_err(|error| {
        format!(
            "Could not read managed aria2 RPC secret at {}: {error}",
            path.display()
        )
    })?;

    validate_managed_rpc_secret(&contents)
}

fn validate_managed_rpc_secret(contents: &str) -> Result<String, String> {
    let secret = contents.trim();

    if secret.len() != 64 || !secret.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(String::from(
            "The managed aria2 RPC secret is missing or malformed",
        ));
    }

    Ok(secret.to_owned())
}

// here lie the tests (generated by AI)
#[cfg(test)]
mod tests {
    use super::{DownloadBackend, ExternalAria2Backend, convert_snapshot_responses};
    use crate::{
        aria2::Aria2DownloadResponse, config::ExternalServerConfig, domain::status::Status,
    };

    #[test]
    fn accepts_a_valid_managed_rpc_secret() {
        let secret = "a".repeat(64);

        assert_eq!(super::validate_managed_rpc_secret(&secret).unwrap(), secret);
    }

    #[test]
    fn rejects_a_malformed_managed_rpc_secret() {
        assert!(super::validate_managed_rpc_secret("not-a-secret").is_err());
        assert!(super::validate_managed_rpc_secret("").is_err());
    }

    fn sample_response(gid: &str, status: &str) -> Aria2DownloadResponse {
        Aria2DownloadResponse {
            gid: gid.to_owned(),
            status: status.to_owned(),
            total_length: String::from("1000"),
            completed_length: String::from("0"),
            download_speed: String::from("0"),
            files: Vec::new(),
            dir: None,
            error_message: None,
        }
    }

    #[test]
    fn valid_snapshot_is_marked_complete() {
        let (downloads, is_complete) =
            convert_snapshot_responses(vec![sample_response("valid-gid", "active")]);

        assert!(is_complete);
        assert_eq!(downloads.len(), 1);
    }

    #[test]
    fn invalid_entry_makes_snapshot_incomplete_but_keeps_valid_downloads() {
        let (downloads, is_complete) = convert_snapshot_responses(vec![
            sample_response("valid-gid", "active"),
            sample_response("invalid-gid", "unknown-status"),
        ]);

        assert!(!is_complete);
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].gid, "valid-gid");
    }

    #[test]
    fn external_backend_refuses_to_delete_download_files() {
        let backend = ExternalAria2Backend::new(
            ExternalServerConfig {
                // This endpoint must never be contacted by this test.
                rpc_url: String::from("http://127.0.0.1:1/jsonrpc"),
                rpc_secret: None,
            },
            false,
        );

        let error = backend
            .remove_download_and_delete_files("0123456789abcdef", &Status::Complete)
            .unwrap_err();

        assert!(error.contains("only available in managed mode"));
    }
}
