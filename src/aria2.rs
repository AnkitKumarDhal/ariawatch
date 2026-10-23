// Handles aria2 JSON-RPC requests and responses
use crate::domain::{download::Download, status::Status};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

// Represents an aria2 JSON-RPC error
#[derive(Debug, Deserialize)]
pub struct Aria2RpcError {
    pub code: i64,
    pub message: String,
}

// Represents an outgoing JSON-RPC request
#[derive(Debug, Serialize)]
pub struct Aria2RpcRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: Vec<serde_json::Value>,
}

// Reprsents a JSON-RPC response envelope
#[derive(Debug, Deserialize)]
pub struct Aria2RpcResponse {
    pub jsonrpc: String,
    pub id: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<Aria2RpcError>,
}

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

pub fn parse_rpc_response(json: &str) -> Result<Aria2RpcResponse, String> {
    serde_json::from_str(json).map_err(|error| format!("Invalid JSON RPC response: {error}"))
}

fn parse_u64_field(field_name: &str, value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("Invalid string for {field_name}: {error}"))
}

pub fn parse_download_list(value: serde_json::Value) -> Result<Vec<Aria2DownloadResponse>, String> {
    serde_json::from_value(value).map_err(|error| format!("Invalid aria2 download list: {error}"))
}

pub fn build_rpc_request(
    method: &str,
    mut params: Vec<serde_json::Value>,
    rpc_secret: Option<&str>,
) -> Result<String, String> {
    if let Some(secret) = rpc_secret {
        params.insert(0, serde_json::Value::String(format!("token:{secret}")));
    }

    let request = Aria2RpcRequest {
        jsonrpc: String::from("2.0"),
        id: String::from("ariawatch-1"),
        method: method.to_owned(),
        params,
    };

    serde_json::to_string(&request)
        .map_err(|error| format!("Could not serialize rpc request: {error}"))
}

pub fn build_get_version_request(rpc_secret: Option<&str>) -> Result<String, String> {
    build_rpc_request("aria2.getVersion", Vec::new(), rpc_secret)
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

// re-usable rpc call method
pub fn call_rpc_method(
    endpoint: &str,
    method: &str,
    params: Vec<serde_json::Value>,
    rpc_secret: Option<&str>,
) -> Result<serde_json::Value, String> {
    let request = build_rpc_request(method, params, rpc_secret)?;
    let body = send_rpc_request(endpoint, &request)?;
    let response = parse_rpc_response(&body)?;
    response.into_result("ariawatch-1")
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

pub fn send_rpc_request(endpoint: &str, request_json: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|error| format!("Could not create HTTP Client: {error}"))?;

    let response = client
        .post(endpoint)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(request_json.to_owned())
        .send()
        .map_err(|error| format!("Could not reach aria2 rpc endpoint: {error}"))?
        .error_for_status()
        .map_err(|error| format!("aria2 returned an HTTP error: {error}"))?;

    response
        .text()
        .map_err(|error| format!("Could not read aria2 response: {error}"))
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

// Fetches all downloads for a full snapshot
pub fn fetch_all_downloads(
    endpoint: &str,
    rpc_secret: Option<&str>,
) -> Result<(usize, usize, usize, Vec<Aria2DownloadResponse>), String> {
    let active_request = build_tell_active_request(rpc_secret)?;
    let active_downloads = fetch_downloads(endpoint, &active_request)?;

    let waiting_request = build_tell_waiting_request(rpc_secret, 0, 1000)?;
    let waiting_downloads = fetch_downloads(endpoint, &waiting_request)?;

    let stopped_request = build_tell_stopped_request(rpc_secret, 0, 1000)?;
    let stopped_downloads = fetch_downloads(endpoint, &stopped_request)?;

    let active_count = active_downloads.len();
    let waiting_count = waiting_downloads.len();
    let stopped_count = stopped_downloads.len();

    let mut all_downloads = active_downloads;
    all_downloads.extend(waiting_downloads);
    all_downloads.extend(stopped_downloads);

    Ok((active_count, waiting_count, stopped_count, all_downloads))
}

impl Aria2RpcResponse {
    // Validates the RPC envelope and extracts it result or error
    pub fn into_result(self, expected_id: &str) -> Result<serde_json::Value, String> {
        if self.jsonrpc != "2.0" {
            return Err(format!("Unexpected JSON-RPC version: {}", self.jsonrpc));
        }

        if self.id != expected_id {
            return Err(format!(
                "Response ID mismatch: expected {expected_id} got {}",
                self.id
            ));
        }

        match (self.result, self.error) {
            (Some(result), None) => Ok(result),
            (None, Some(error)) => {
                Err(format!("aria2 rpc error {}: {}", error.code, error.message))
            }
            (Some(_), Some(_)) => Err(String::from(
                "Invalid RPC response: both result and error fields populated",
            )),
            (None, None) => Err(String::from(
                "Invalid RPC response: neither result nor error fields populated",
            )),
        }
    }
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

#[cfg(test)]
mod control_tests {
    use super::{build_add_torrent_request, build_add_uri_request, validate_gid};
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
}
