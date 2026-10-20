use std::sync::mpsc::Receiver;

use crate::{
    aria2,
    config::{ExternalServerConfig, RuntimeConfig, RuntimeMode},
    domain::download::Download,
    logging,
    monitor::{self, MonitorUpdate},
};

pub struct DownloadSnapshot {
    pub active_count: usize,
    pub waiting_count: usize,
    pub stopped_count: usize,
    pub downloads: Vec<Download>,
}

pub trait DownloadBackend {
    fn get_version(&self) -> Result<String, String>;
    fn fetch_snapshot(&self) -> Result<DownloadSnapshot, String>;
    fn start_montior(
        &self,
        initial_active_gids: Vec<String>,
    ) -> Result<Receiver<MonitorUpdate>, String>;
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

    fn start_montior(
        &self,
        initial_active_gids: Vec<String>,
    ) -> Result<Receiver<MonitorUpdate>, String> {
        monitor::start(
            self.config.rpc_url.clone(),
            self.config.rpc_secret.clone(),
            initial_active_gids,
        )
    }
}

pub fn create_backend(config: RuntimeConfig) -> Result<Box<dyn DownloadBackend>, String> {
    match config.mode {
        RuntimeMode::Managed => Err("Managed mode is not implemented yet. The background controller will provide this backend.".to_owned()),
        RuntimeMode::External(external) => {
            Ok(Box::new(ExternalAria2Backend::new(external)))
        }
    }
}
