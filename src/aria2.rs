// Handles aria2 JSON-RPC requests and responses
mod rpc;

use crate::domain::{download::Download, status::Status};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
pub use rpc::{
    build_get_version_request, build_rpc_request, call_rpc_method, parse_rpc_response,
    send_rpc_request,
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

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

// normally RPC request allows a max of 2.0 MiB of transfer, so limiting it to 1.4MB is a good way
// to not hit these limits
const MAX_TORRENT_BYTES: u64 = 1_400_000;

// does the request for adding a download link (even supports magnets)
pub fn build_add_uri_request(uri: &str, rpc_secret: Option<&str>) -> Result<String, String> {
    let uri = uri.trim();

    if uri.is_empty() || uri.chars().any(char::is_whitespace) {
        return Err(String::from("Download URI is empty or contains whitespace"));
    }

    let lower = uri.to_ascii_lowercase();
    let supported = [
        "http://", "https://", "ftp://", "ftps://", "sftp://", "magnet:?",
    ];

    if !supported.iter().any(|prefix| lower.starts_with(prefix)) {
        return Err(String::from(
            "Unsupported URI scheme; use HTTP(s), FTP(s), SFTP or a magnet URI",
        ));
    }

    build_rpc_request("aria2.addUri", vec![serde_json::json!([uri])], rpc_secret)
}

// does the same thing as above, but for .torrent files
pub fn build_add_torrent_request(
    contents: &[u8],
    rpc_secret: Option<&str>,
) -> Result<String, String> {
    if contents.is_empty() {
        return Err(String::from("Torrent file is empty"));
    }

    if contents.len() as u64 > MAX_TORRENT_BYTES {
        return Err(String::from("Torrent file exceeds the 1.4MB upload limit"));
    }

    let encoded = BASE64.encode(contents);

    build_rpc_request(
        "aria2.addTorrent",
        vec![serde_json::json!(encoded)],
        rpc_secret,
    )
}

// checks the emptyness and type of the returned rpc result and then converst the answer (which is a
// GID) into a string that contains the GID
fn expect_gid(result: serde_json::Value, method: &str) -> Result<String, String> {
    result
        .as_str()
        .filter(|gid| !gid.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("{method} returned as an invalid GID"))
}

// automatically takes in a URI / magnet and builds the rpc request and submits it to aria2
pub fn add_uri(endpoint: &str, uri: &str, rpc_secret: Option<&str>) -> Result<String, String> {
    let request = build_add_uri_request(uri, rpc_secret)?;
    let body = send_rpc_request(endpoint, &request)?;
    let response = parse_rpc_response(&body)?;
    let result = response.into_result("ariawatch-1")?;

    expect_gid(result, "aria2.addUri")
}

// does the same as above but with .torrent files
pub fn add_torrent_file(
    endpoint: &str,
    path: &Path,
    rpc_secret: Option<&str>,
) -> Result<String, String> {
    let is_torrent = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("torrent"));

    if !is_torrent {
        return Err(String::from("Select a local .torrent file"));
    }

    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not inspect torrent file {}: {error}", path.display()))?;

    if !metadata.is_file() {
        return Err(String::from("Torrent path is not a regular file"));
    }

    if metadata.len() == 0 || metadata.len() > MAX_TORRENT_BYTES {
        return Err(String::from(
            "Torrent file must be non-empty and no larger than 1.4MB",
        ));
    }

    let contents = fs::read(path)
        .map_err(|error| format!("Could not read torrent file {}: {error}", path.display()))?;

    let request = build_add_torrent_request(&contents, rpc_secret)?;
    let body = send_rpc_request(endpoint, &request)?;
    let response = parse_rpc_response(&body)?;
    let result = response.into_result("ariawatch-1")?;

    expect_gid(result, "aria2.addTorrent")
}

// this does the validation part of the GID that is provided to ariawatch
// NOTE: this function validtes gid that is *provided to ariawatch* while the previous function
// validated the GID that was *returned by aria2*
fn validate_gid(gid: &str) -> Result<(), String> {
    if gid.len() != 16 || !gid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(String::from("Invalid aria2 GID"));
    }

    Ok(())
}

// tthis is the helper function for the functions taht target one single download by GID
fn run_gid_action(
    endpoint: &str,
    method: &str,
    gid: &str,
    rpc_secret: Option<&str>,
) -> Result<(), String> {
    validate_gid(gid)?;

    let result = call_rpc_method(endpoint, method, vec![serde_json::json!(gid)], rpc_secret)?;

    match result.as_str() {
        Some(returned_gid) if returned_gid == gid => Ok(()),
        _ => Err(format!("{method} returned an unexpected result")),
    }
}

// download actions
pub fn pause_download(endpoint: &str, gid: &str, rpc_secret: Option<&str>) -> Result<(), String> {
    run_gid_action(endpoint, "aria2.pause", gid, rpc_secret)
}

pub fn resume_download(endpoint: &str, gid: &str, rpc_secret: Option<&str>) -> Result<(), String> {
    run_gid_action(endpoint, "aria2.unpause", gid, rpc_secret)
}

// this one is for the functions that target all the downloads at once
pub fn run_all_action(
    endpoint: &str,
    method: &str,
    rpc_secret: Option<&str>,
) -> Result<(), String> {
    let result = call_rpc_method(endpoint, method, Vec::new(), rpc_secret)?;

    if result.as_str() == Some("OK") {
        Ok(())
    } else {
        Err(format!("{method} retuned an unexpected result"))
    }
}

pub fn pause_all(endpoint: &str, rpc_secret: Option<&str>) -> Result<(), String> {
    run_all_action(endpoint, "aria2.pauseAll", rpc_secret)
}

pub fn resume_all(endpoint: &str, rpc_secret: Option<&str>) -> Result<(), String> {
    run_all_action(endpoint, "aria2.unpauseAll", rpc_secret)
}

fn remove_download_result(
    endpoint: &str,
    gid: &str,
    rpc_secret: Option<&str>,
) -> Result<(), String> {
    let result = call_rpc_method(
        endpoint,
        "aria2.removeDownloadResult",
        vec![serde_json::json!(gid)],
        rpc_secret,
    )?;

    if result.as_str() == Some("OK") {
        Ok(())
    } else {
        Err(String::from(
            "aria2.removeDownloadResult returned an unexpected result",
        ))
    }
}

pub fn remove_download(
    endpoint: &str,
    gid: &str,
    status: &Status,
    rpc_secret: Option<&str>,
) -> Result<(), String> {
    validate_gid(gid)?;

    match status {
        Status::Active => {
            let result = call_rpc_method(
                endpoint,
                "aria2.remove",
                vec![serde_json::json!(gid)],
                rpc_secret,
            )?;

            if result.as_str() != Some(gid) {
                return Err(String::from("aria2.remove returned an unexpected result"));
            }

            remove_download_result(endpoint, gid, rpc_secret)
        }

        Status::Waiting | Status::Paused => {
            let result = call_rpc_method(
                endpoint,
                "aria2.remove",
                vec![serde_json::json!(gid)],
                rpc_secret,
            )?;

            if result.as_str() != Some(gid) {
                return Err(String::from("aria2.remove returned an unexpected result"));
            }

            Ok(())
        }

        Status::Complete | Status::Error | Status::Removed => {
            remove_download_result(endpoint, gid, rpc_secret)
        }
    }
}

// helper function to help parse the download file locations before giving it to the get_download_file_paths()
fn parse_download_file_paths(
    value: &serde_json::Value,
    source: &str,
) -> Result<Vec<PathBuf>, String> {
    let files = value
        .as_array()
        .ok_or_else(|| format!("{source} returned an invlaid file list: expected an array"))?;

    if files.is_empty() {
        return Err(format!(
            "{source} returned an empty file list; refusing to delete files"
        ));
    }

    let mut paths = Vec::with_capacity(files.len());

    for (index, file) in files.iter().enumerate() {
        let object = file
            .as_object()
            .ok_or_else(|| format!("{source} returned a non-object entry for file {index}"))?;
        let path_value = object.get("path").ok_or_else(|| {
            let fields = object.keys().map(|key| key.as_str()).collect::<Vec<_>>().join(", ");
            format!("{source} returned an invalid path for file {index}: missing `path` field (available fields: {fields})")
        })?;

        let path = path_value.as_str().filter(|path| !path.trim().is_empty()).ok_or_else(|| {
            let actual_type = match path_value {
                serde_json::Value::Null => "null",
                serde_json::Value::Bool(_) => "boolean",
                serde_json::Value::Number(_) => "number",
                serde_json::Value::String(_) => "empty or whitespace-only string",
                serde_json::Value::Array(_) => "array",
                serde_json::Value::Object(_) => "object"
            };

            format!("{source} returned an invalid path for the file {index}: expected a non-empty string, got {actual_type}")
        })?;

        paths.push(PathBuf::from(path));
    }

    Ok(paths)
}

// figures out the file path to be deleted
pub fn get_download_file_paths(
    endpoint: &str,
    gid: &str,
    rpc_secret: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    validate_gid(gid)?;

    let primary = call_rpc_method(
        endpoint,
        "aria2.getFiles",
        vec![serde_json::json!(gid)],
        rpc_secret,
    )
    .and_then(|value| parse_download_file_paths(&value, "aria2.getFiles"));

    match primary {
        Ok(paths) => Ok(paths),
        Err(primary_error) => {
            let fallback = call_rpc_method(
                endpoint,
                "aria2.tellStatus",
                vec![serde_json::json!(gid)],
                rpc_secret,
            )
            .and_then(|status| {
                status
                    .get("files")
                    .cloned()
                    .ok_or_else(|| String::from("aria2.tellStatus returned no `files` field"))
            })
            .and_then(|files| parse_download_file_paths(&files, "aria2.tellStatus.files"));

            match fallback {
                Ok(paths) => Ok(paths),
                Err(fallback_error) => Err(format!(
                    "Coult no retrieve file paths safely. aria2.getFiles failed: {primary_error}. aria2.tellStatus fallback failed: {fallback_error}"
                )),
            }
        }
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

#[cfg(test)]
mod control_tests {
    use super::{
        build_add_torrent_request, build_add_uri_request, parse_download_file_paths, validate_gid,
    };
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

    #[test]
    fn builds_authenticated_magnet_request() {
        let request =
            build_add_uri_request("magnet:?xt=urn:btih:0123456789abcdef", Some("test-secret"))
                .unwrap();

        let value: serde_json::Value = serde_json::from_str(&request).unwrap();

        assert_eq!(value["method"], "aria2.addUri");
        assert_eq!(value["params"][0], "token:test-secret");
        assert_eq!(
            value["params"][1][0],
            "magnet:?xt=urn:btih:0123456789abcdef"
        );
    }

    #[test]
    fn builds_base64_torrent_request() {
        let contents = b"fake torrent contents";

        let request = build_add_torrent_request(contents, Some("test-secret")).unwrap();

        let value: serde_json::Value = serde_json::from_str(&request).unwrap();

        assert_eq!(value["method"], "aria2.addTorrent");
        assert_eq!(value["params"][0], "token:test-secret");

        let encoded = value["params"][1].as_str().unwrap();
        assert_eq!(BASE64.decode(encoded).unwrap(), contents);
    }

    #[test]
    fn rejects_invalid_gids() {
        assert!(validate_gid("short").is_err());
        assert!(validate_gid("zzzzzzzzzzzzzzzz").is_err());
        assert!(validate_gid("0123456789abcdef").is_ok());
    }

    #[test]
    fn rejects_unsupported_uri_schemes() {
        assert!(build_add_uri_request("file:///etc/passwd", None).is_err());
        assert!(build_add_uri_request("not a URI", None).is_err());
    }

    #[test]
    fn parses_file_paths_for_download_removal() {
        let value = serde_json::json!([
            {
                "index": "1",
                "path": "/home/test/Downloads/file.iso"
            },
            {
                "index": "2",
                "path": "/home/test/Downloads/subdir/file.txt"
            }
        ]);

        let paths = parse_download_file_paths(&value, "aria2.getFiles").unwrap();

        assert_eq!(
            paths,
            vec![
                std::path::PathBuf::from("/home/test/Downloads/file.iso"),
                std::path::PathBuf::from("/home/test/Downloads/subdir/file.txt"),
            ]
        );
    }

    #[test]
    fn rejects_empty_file_list_for_deletion() {
        let error =
            parse_download_file_paths(&serde_json::json!([]), "aria2.getFiles").unwrap_err();

        assert!(error.contains("empty file list"));
    }

    #[test]
    fn reports_missing_file_path_field() {
        let error = parse_download_file_paths(
            &serde_json::json!([
                {"index": "1", "length": "123"}
            ]),
            "aria2.getFiles",
        )
        .unwrap_err();

        assert!(error.contains("missing `path` field"));
        assert!(error.contains("index"));
    }

    #[test]
    fn reports_non_string_file_paths() {
        let error = parse_download_file_paths(
            &serde_json::json!([
                {"index": "1", "path": null}
            ]),
            "aria2.getFiles",
        )
        .unwrap_err();

        assert!(error.contains("expected a non-empty string"));
        assert!(error.contains("null"));
    }
}
