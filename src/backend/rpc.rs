use super::{DownloadBackend, DownloadSnapshot, FileDeleteReport};
use crate::config::ExternalServerConfig;
use crate::domain::download::Download;
use crate::domain::status::Status;
use crate::monitor::{self, MonitorUpdate};
use crate::{aria2, logging};
use std::fs;
use std::path::Path;
use std::sync::mpsc::Receiver;

pub(super) struct ExternalAria2Backend {
    config: ExternalServerConfig,
    managed_mode: bool,
}

impl ExternalAria2Backend {
    pub(super) fn new(config: ExternalServerConfig, managed_mode: bool) -> Self {
        Self {
            config,
            managed_mode,
        }
    }
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

        let directory = super::file_safety::managed_download_dir()?;
        let file_paths = aria2::get_download_file_paths(
            &self.config.rpc_url,
            gid,
            self.config.rpc_secret.as_deref(),
        )?;
        let safe_files = super::file_safety::collect_safe_files(file_paths, &directory)?;

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

// here lie the tests (generated by AI)
#[cfg(test)]
mod tests {
    use super::{DownloadBackend, ExternalAria2Backend, convert_snapshot_responses};
    use crate::{
        aria2::Aria2DownloadResponse, config::ExternalServerConfig, domain::status::Status,
    };

    fn sample_response(gid: &str, status: &str) -> Aria2DownloadResponse {
        serde_json::from_value(serde_json::json!({
            "gid": gid,
            "status": status,
            "totalLength": "1000",
            "completedLength": "0",
            "downloadSpeed": "0",
            "files": [],
            "dir": null,
            "errorMessage": null
        }))
        .expect("valid test response")
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
