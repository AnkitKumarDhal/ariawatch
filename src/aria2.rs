// Handles aria2 JSON-RPC requests and responses
mod operations;
mod rpc;
use crate::domain::{download::Download, status::Status};
pub use operations::{
    add_torrent_file, add_uri, get_download_file_paths, pause_all, pause_download, remove_download,
    resume_all, resume_download,
};
pub use rpc::{build_get_version_request, build_rpc_request, parse_rpc_response, send_rpc_request};
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

// Represents a URI associated with an aria2 file
#[derive(Debug, Deserialize)]
pub struct Aria2UriResponse {
    pub uri: String,
    pub status: String,
}

// Represents a file entry in an aria2 response
#[derive(Debug, Deserialize)]
pub struct Aria2FileResponse {
    pub path: String,
    #[serde(default)]
    pub uris: Vec<Aria2UriResponse>,
}

// Represents download data returned by aria2
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aria2DownloadResponse {
    pub gid: String,
    pub status: String,
    pub total_length: String,
    pub completed_length: String,
    pub download_speed: String,
    pub files: Vec<Aria2FileResponse>,

    #[serde(default)]
    pub dir: Option<String>,

    #[serde(default)]
    pub error_message: Option<String>,
}

fn parse_u64_field(field_name: &str, value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("Invalid string for {field_name}: {error}"))
}

pub fn parse_download_list(value: serde_json::Value) -> Result<Vec<Aria2DownloadResponse>, String> {
    serde_json::from_value(value).map_err(|error| format!("Invalid aria2 download list: {error}"))
}

impl Aria2DownloadResponse {
    // Converts aria2 response into AriaWatch's internal model
    pub fn into_download(self) -> Result<Download, String> {
        let name = self
            .files
            .first()
            .and_then(|file| Path::new(&file.path).file_name())
            .and_then(|name| name.to_str())
            .map(String::from)
            .unwrap_or_else(|| self.gid.clone());

        let source_url = self
            .files
            .iter()
            .flat_map(|file| file.uris.iter())
            .find(|uri| uri.status.as_str() == "used")
            .or_else(|| self.files.iter().flat_map(|file| file.uris.iter()).next())
            .map(|uri| uri.uri.clone());

        let total_length = parse_u64_field("totalLength", &self.total_length)?;
        let completed_length = parse_u64_field("completedLength", &self.completed_length)?;
        let download_speed = parse_u64_field("downloadSpeed", &self.download_speed)?;
        let status = Status::from_aria2(&self.status)?;

        Ok(Download::new(
            self.gid,
            name,
            total_length,
            completed_length,
            download_speed,
            status,
            self.dir,
            source_url,
            self.error_message,
        ))
    }
}

// Ask aria2 only for fields used by the current table
fn download_keys() -> serde_json::Value {
    serde_json::json!([
        "gid",
        "status",
        "totalLength",
        "completedLength",
        "downloadSpeed",
        "files",
        "dir",
        "errorMessage"
    ])
}

pub fn build_tell_active_request(rpc_secret: Option<&str>) -> Result<String, String> {
    build_rpc_request("aria2.tellActive", vec![download_keys()], rpc_secret)
}

pub fn build_tell_waiting_request(
    rpc_secret: Option<&str>,
    offset: i64,
    num: i64,
) -> Result<String, String> {
    build_rpc_request(
        "aria2.tellWaiting",
        vec![
            serde_json::json!(offset),
            serde_json::json!(num),
            download_keys(),
        ],
        rpc_secret,
    )
}

pub fn build_tell_stopped_request(
    rpc_secret: Option<&str>,
    offset: i64,
    num: i64,
) -> Result<String, String> {
    build_rpc_request(
        "aria2.tellStopped",
        vec![
            serde_json::json!(offset),
            serde_json::json!(num),
            download_keys(),
        ],
        rpc_secret,
    )
}

// Fetches and parses the list of downloads returned by the RPC request
pub fn fetch_downloads(
    endpoint: &str,
    request_json: &str,
) -> Result<Vec<Aria2DownloadResponse>, String> {
    let response_json = send_rpc_request(endpoint, request_json)?;
    let rpc_response = parse_rpc_response(&response_json)?;
    let result = rpc_response.into_result("ariawatch-1")?;
    parse_download_list(result)
}

const DOWNLOAD_PAGE_SIZE: i64 = 1000;

// this brings paginated download fetching (it was easy to implement ngl)
fn fetch_paged_downloads(
    endpoint: &str,
    rpc_secret: Option<&str>,
    stopped: bool,
) -> Result<Vec<Aria2DownloadResponse>, String> {
    let kind = if stopped { "stopped" } else { "waiting" };
    let mut offset = 0_i64;
    let mut downloads = Vec::new();

    loop {
        let request = if stopped {
            build_tell_stopped_request(rpc_secret, offset, DOWNLOAD_PAGE_SIZE)?
        } else {
            build_tell_waiting_request(rpc_secret, offset, DOWNLOAD_PAGE_SIZE)?
        };

        let page = fetch_downloads(endpoint, &request)?;
        let page_length = page.len();
        downloads.extend(page);

        if page_length < DOWNLOAD_PAGE_SIZE as usize {
            break;
        }

        offset = offset
            .checked_add(DOWNLOAD_PAGE_SIZE)
            .ok_or_else(|| format!("aria2 {kind} pagination offset overflow"))?;
    }

    Ok(downloads)
}

// this merges the paginated downloads that were fetched in chunks
fn merge_download_pages(
    active: Vec<Aria2DownloadResponse>,
    waiting: Vec<Aria2DownloadResponse>,
    stopped: Vec<Aria2DownloadResponse>,
) -> Vec<Aria2DownloadResponse> {
    let mut positions: HashMap<String, usize> = HashMap::new();
    let mut downloads: Vec<Aria2DownloadResponse> = Vec::new();

    for download in active.into_iter().chain(waiting).chain(stopped) {
        if let Some(index) = positions.get(download.gid.as_str()).copied() {
            downloads[index] = download;
        } else {
            positions.insert(download.gid.clone(), downloads.len());
            downloads.push(download);
        }
    }

    downloads
}

// Fetches all downloads for a full snapshot
pub fn fetch_all_downloads(
    endpoint: &str,
    rpc_secret: Option<&str>,
) -> Result<(usize, usize, usize, Vec<Aria2DownloadResponse>), String> {
    let active_request = build_tell_active_request(rpc_secret)?;
    let active_downloads = fetch_downloads(endpoint, &active_request)?;

    let waiting_downloads = fetch_paged_downloads(endpoint, rpc_secret, false)?;
    let stopped_downloads = fetch_paged_downloads(endpoint, rpc_secret, true)?;

    let active_count = active_downloads.len();
    let waiting_count = waiting_downloads.len();
    let stopped_count = stopped_downloads.len();

    let all_downloads =
        merge_download_pages(active_downloads, waiting_downloads, stopped_downloads);

    Ok((active_count, waiting_count, stopped_count, all_downloads))
}
