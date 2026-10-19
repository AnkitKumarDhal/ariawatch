// Handles aria2 JSON-RPC requests and responses
use crate::domain::{download::Download, status::Status};
use serde::{Deserialize, Serialize};
use std::path::Path;

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
        "error_message"
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
