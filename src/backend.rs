use crate::{
    aria2,
    config::{DEFAULT_RPC_URL, ExternalServerConfig, RuntimeConfig, RuntimeMode},
    controller,
    domain::download::Download,
    logging,
    monitor::{self, MonitorUpdate},
    paths::AppPaths,
};
use std::{fs, path::Path, sync::mpsc::Receiver};

pub struct DownloadSnapshot {
    pub active_count: usize,
    pub waiting_count: usize,
    pub stopped_count: usize,
    pub downloads: Vec<Download>,
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
}

struct ExternalAria2Backend {
    config: ExternalServerConfig,
}

impl ExternalAria2Backend {
    fn new(config: ExternalServerConfig) -> Self {
        Self { config }
    }
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

        let mut downloads = Vec::with_capacity(responses.len());

        for response in responses {
            match response.into_download() {
                Ok(download) => downloads.push(download),
                Err(error) => {
                    logging::warn(&format!("Skipping invalid download: {error}"));
                }
            }
        }

        Ok(DownloadSnapshot {
            active_count,
            waiting_count,
            stopped_count,
            downloads,
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

            Ok(Box::new(ExternalAria2Backend::new(server)))
        }

        RuntimeMode::External(external) => Ok(Box::new(ExternalAria2Backend::new(external))),
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

#[cfg(test)]
mod tests {
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
}
