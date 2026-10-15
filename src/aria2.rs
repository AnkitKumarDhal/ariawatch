use crate::domain::{download::Download, status::Status};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Aria2RpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct Aria2RpcRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct Aria2RpcResponse {
    pub jsonrpc: String,
    pub id: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<Aria2RpcError>,
}

#[derive(Debug, Deserialize)]
pub struct Aria2FileResponse {
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aria2DownloadResponse {
    pub gid: String,
    pub status: String,
    pub total_length: String,
    pub completed_length: String,
    pub download_speed: String,
    pub files: Vec<Aria2FileResponse>,
}

pub fn parse_rpc_response(json: &str) -> Result<Aria2RpcResponse, String> {
    serde_json::from_str(json).map_err(|error| format!("Invalid JSON RPC response: {error}"))
}

pub fn parse_download_response(json: &str) -> Result<Aria2DownloadResponse, String> {
    serde_json::from_str(json).map_err(|error| format!("Invalid aria2 JSON response: {error}"))
}

pub fn parse_u64_field(field_name: &str, value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("Invalid string for {field_name}: {error}"))
}

pub fn build_get_version_request() -> Result<String, String> {
    let request = Aria2RpcRequest {
        jsonrpc: String::from("2.0"),
        id: String::from("ariawatch-1"),
        method: String::from("aria2.getVersion"),
        params: Vec::new(),
    };

    serde_json::to_string(&request)
        .map_err(|error| format!("Could not serialize JSON RPC request: {error}"))
}

impl Aria2RpcResponse {
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
            (Some(_), Some(_)) => Err(format!(
                "Invalid RPC response: both result and error fields populated"
            )),
            (None, None) => Err(format!(
                "Invalid RPC response: neither result nor error fields populated"
            )),
        }
    }
}

impl Aria2DownloadResponse {
    pub fn into_download(self) -> Result<Download, String> {
        let name = self
            .files
            .first()
            .and_then(|file| Path::new(&file.path).file_name())
            .and_then(|name| name.to_str())
            .map(String::from)
            .unwrap_or_else(|| self.gid.clone());

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
        ))
    }
}
