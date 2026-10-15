use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aria2DownloadResponse {
    pub gid: String,
    pub status: String,
    pub total_length: String,
    pub completed_length: String,
    pub download_speed: String,
}

pub fn parse_download_response(json: &str) -> Result<Aria2DownloadResponse, String> {
    serde_json::from_str(json).map_err(|error| format!("Invalid aria2 JSON response: {error}"))
}

pub fn parse_u64_field(field_name: &str, value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("Invalid string for {field_name}: {error}"))
}
