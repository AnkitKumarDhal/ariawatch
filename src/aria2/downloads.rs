use super::rpc::{build_rpc_request, parse_rpc_response, send_rpc_request};
use crate::domain::{
    download::{Download, DownloadFile, TorrentStats},
    status::Status,
};
use serde::Deserialize;
use std::{collections::HashMap, path::Path};

// Represents a URI associated with an aria2 file
#[derive(Debug, Deserialize)]
struct Aria2UriResponse {
    pub uri: String,
    pub status: String,
}

// Represents a file entry in an aria2 response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Aria2FileResponse {
    pub path: String,
    pub length: Option<String>,
    pub completed_length: Option<String>,
    #[serde(default)]
    pub uris: Vec<Aria2UriResponse>,
}

// this is the inner info dictionary of a torrent download
#[derive(Debug, Deserialize)]
struct Aria2BitTorrentInfoResponse {
    #[serde(default)]
    name: Option<String>,
}

// this is the bittorrent metadata returned by aria2
#[derive(Debug, Deserialize)]
struct Aria2BitTorrentResponse {
    #[serde(default)]
    mode: Option<String>,

    #[serde(default)]
    info: Option<Aria2BitTorrentInfoResponse>,
}

// Represents download data returned by aria2
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aria2DownloadResponse {
    pub gid: String,
    status: String,
    total_length: String,
    completed_length: String,
    download_speed: String,
    files: Vec<Aria2FileResponse>,

    #[serde(default)]
    bittorrent: Option<Aria2BitTorrentResponse>,

    #[serde(default)]
    dir: Option<String>,

    #[serde(default)]
    error_message: Option<String>,

    #[serde(default)]
    upload_length: Option<String>,

    #[serde(default)]
    upload_speed: Option<String>,

    #[serde(default)]
    info_hash: Option<String>,

    #[serde(default)]
    num_seeders: Option<String>,

    #[serde(default)]
    seeder: Option<String>,

    #[serde(default)]
    connections: Option<String>,
}

fn parse_u64_field(field_name: &str, value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("Invalid string for {field_name}: {error}"))
}

// parses an optional numeric field without treating missing values as 0
fn parse_optional_u64_field(field_name: &str, value: Option<&str>) -> Result<Option<u64>, String> {
    value
        .map(|value| parse_u64_field(field_name, value))
        .transpose()
}

fn parse_download_list(value: serde_json::Value) -> Result<Vec<Aria2DownloadResponse>, String> {
    serde_json::from_value(value).map_err(|error| format!("Invalid aria2 download list: {error}"))
}

// finds the actual directory used by the multi-file torrents,,, aria2's dir is the base directory
// and then the torrent directory is added to it
fn resolve_torrent_dir(
    base_dir: Option<&str>,
    torrent_mode: Option<&str>,
    files: &[DownloadFile],
) -> Option<String> {
    let base_dir = base_dir?;
    if torrent_mode != Some("multi") {
        return Some(base_dir.to_owned());
    }

    let base_path = Path::new(base_dir);

    let torrent_dir = files.iter().find_map(|file| {
        let relative_path = Path::new(&file.path).strip_prefix(base_path).ok()?;

        match relative_path.components().next()? {
            std::path::Component::Normal(component) => Some(component.to_os_string()),
            _ => None,
        }
    })?;

    Some(base_path.join(torrent_dir).to_string_lossy().into_owned())
}

impl Aria2DownloadResponse {
    // Converts aria2 response into AriaWatch's internal model
    pub fn into_download(self) -> Result<Download, String> {
        // get the actual torrent name from the bittorrent data
        let torrent_name = self
            .bittorrent
            .as_ref()
            .and_then(|torrent| torrent.info.as_ref())
            .and_then(|info| info.name.as_deref())
            .filter(|name| !name.trim().is_empty())
            .map(str::to_owned);

        let torrent_mode = self
            .bittorrent
            .as_ref()
            .and_then(|torrent| torrent.mode.as_deref());

        // prefer torrent metadata name,,, fall back to the first file name for normal downlaods or
        // when aria2 does not provide the torrent metadata
        let name = torrent_name.unwrap_or_else(|| {
            self.files
                .first()
                .and_then(|file| Path::new(&file.path).file_name())
                .and_then(|name| name.to_str())
                .map(String::from)
                .unwrap_or_else(|| self.gid.clone())
        });

        let source_url = self
            .files
            .iter()
            .flat_map(|file| file.uris.iter())
            .find(|uri| uri.status.as_str() == "used")
            .or_else(|| self.files.iter().flat_map(|file| file.uris.iter()).next())
            .map(|uri| uri.uri.clone());

        let mut files = Vec::with_capacity(self.files.len());

        for file in self.files {
            let size = match file.length.as_deref() {
                Some(value) => parse_u64_field("file length", value)?,
                None => 0,
            };

            let downloaded = match file.completed_length.as_deref() {
                Some(value) => parse_u64_field("file completedLength", value)?,
                None => 0,
            };

            files.push(DownloadFile {
                path: file.path,
                size,
                downloaded,
            });
        }

        let total_length = parse_u64_field("totalLength", &self.total_length)?;
        let completed_length = parse_u64_field("completedLength", &self.completed_length)?;
        let download_speed = parse_u64_field("downloadSpeed", &self.download_speed)?;

        let torrent = match self.info_hash.as_deref() {
            Some(_) => {
                let seeders = parse_optional_u64_field("numSeeders", self.num_seeders.as_deref())?;

                let connections =
                    parse_optional_u64_field("connections", self.connections.as_deref())?;

                let peers = connections
                    .zip(seeders)
                    .map(|(connections, seeders)| connections.saturating_sub(seeders));

                Some(TorrentStats {
                    seeders,
                    peers,
                    upload_speed: parse_optional_u64_field(
                        "uploadSpeed",
                        self.upload_speed.as_deref(),
                    )?,
                    uploaded: parse_optional_u64_field(
                        "uploadLength",
                        self.upload_length.as_deref(),
                    )?,
                })
            }
            None => None,
        };

        let status = if self.status == "active"
            && torrent.is_some()
            && self.seeder.as_deref() == Some("true")
        {
            Status::Seeding
        } else {
            Status::from_aria2(&self.status)?
        };

        let directory = resolve_torrent_dir(self.dir.as_deref(), torrent_mode, &files).or(self.dir);

        Ok(Download {
            gid: self.gid,
            name,
            size: total_length,
            downloaded: completed_length,
            speed: download_speed,
            status,
            files,
            torrent,
            directory,
            source_url,
            error_message: self.error_message,
        })
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
        "uploadLength",
        "uploadSpeed",
        "infoHash",
        "numSeeders",
        "seeder",
        "connections",
        "bittorrent",
        "files",
        "dir",
        "errorMessage"
    ])
}

pub fn build_tell_active_request(rpc_secret: Option<&str>) -> Result<String, String> {
    build_rpc_request("aria2.tellActive", vec![download_keys()], rpc_secret)
}

fn build_tell_waiting_request(
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

fn build_tell_stopped_request(
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
mod tests {
    use super::Aria2DownloadResponse;
    use crate::domain::{download::DownloadFile, status::Status};

    #[test]
    fn preserves_all_files_from_aria2_response() {
        let response: Aria2DownloadResponse = serde_json::from_value(serde_json::json!({
            "gid": "0123456789abcdef",
            "status": "active",
            "totalLength": "3072",
            "completedLength": "1024",
            "downloadSpeed": "128",
            "dir": "/home/test/Downloads",
            "files": [
                {
                    "path": "/home/test/Downloads/Example/one.mkv",
                    "length": "1024",
                    "completedLength": "512",
                    "uris": []
                },
                {
                    "path": "/home/test/Downloads/Example/two.mkv",
                    "length": "2048",
                    "completedLength": "512",
                    "uris": []
                }
            ]
        }))
        .unwrap();

        let download = response.into_download().unwrap();

        assert_eq!(download.files.len(), 2);

        assert_eq!(
            download.files,
            vec![
                DownloadFile {
                    path: String::from("/home/test/Downloads/Example/one.mkv"),
                    size: 1024,
                    downloaded: 512,
                },
                DownloadFile {
                    path: String::from("/home/test/Downloads/Example/two.mkv"),
                    size: 2048,
                    downloaded: 512,
                },
            ]
        );
    }

    #[test]
    fn identifies_a_seeding_torrent_and_preserves_its_statistics() {
        let response: Aria2DownloadResponse = serde_json::from_value(serde_json::json!({
            "gid": "0123456789abcdef",
            "status": "active",
            "totalLength": "1048576",
            "completedLength": "1048576",
            "downloadSpeed": "0",
            "uploadLength": "524288",
            "uploadSpeed": "4096",
            "infoHash": "0123456789abcdef0123456789abcdef01234567",
            "numSeeders": "3",
            "seeder": "true",
            "connections": "10",
            "files": []
        }))
        .unwrap();

        let download = response.into_download().unwrap();

        assert_eq!(download.status, Status::Seeding);

        let torrent = download.torrent.unwrap();

        assert_eq!(torrent.seeders, Some(3));
        assert_eq!(torrent.peers, Some(7));
        assert_eq!(torrent.upload_speed, Some(4096));
        assert_eq!(torrent.uploaded, Some(524288));
    }

    #[test]
    fn downloading_torrent_remains_active() {
        let response: Aria2DownloadResponse = serde_json::from_value(serde_json::json!({
            "gid": "0123456789abcdef",
            "status": "active",
            "totalLength": "1048576",
            "completedLength": "524288",
            "downloadSpeed": "1024",
            "uploadLength": "1024",
            "uploadSpeed": "256",
            "infoHash": "0123456789abcdef0123456789abcdef01234567",
            "numSeeders": "2",
            "seeder": "false",
            "connections": "8",
            "files": []
        }))
        .unwrap();

        let download = response.into_download().unwrap();

        assert_eq!(download.status, Status::Active);
        assert_eq!(download.torrent.as_ref().unwrap().peers, Some(6));
    }

    #[test]
    fn non_torrent_download_does_not_get_torrent_statistics() {
        let response: Aria2DownloadResponse = serde_json::from_value(serde_json::json!({
            "gid": "0123456789abcdef",
            "status": "active",
            "totalLength": "1000",
            "completedLength": "500",
            "downloadSpeed": "128",
            "files": []
        }))
        .unwrap();

        let download = response.into_download().unwrap();

        assert_eq!(download.status, Status::Active);
        assert!(download.torrent.is_none());
    }

    #[test]
    fn uses_torrent_metadata_name_and_directory_for_multi_file_torrents() {
        let response: Aria2DownloadResponse = serde_json::from_value(serde_json::json!({
            "gid": "0123456789abcdef",
            "status": "active",
            "totalLength": "3072",
            "completedLength": "1024",
            "downloadSpeed": "128",
            "dir": "/home/test/Downloads",
            "bittorrent": {
                "mode": "multi",
                "info": {
                    "name": "Example Torrent"
                }
            },
            "files": [
                {
                    "path": "/home/test/Downloads/Example Torrent/one.mkv",
                    "length": "1024",
                    "completedLength": "512",
                    "uris": []
                },
                {
                    "path": "/home/test/Downloads/Example Torrent/Disc 1/two.mkv",
                    "length": "2048",
                    "completedLength": "512",
                    "uris": []
                }
            ]
        }))
        .unwrap();

        let download = response.into_download().unwrap();

        assert_eq!(download.name, "Example Torrent");

        assert_eq!(
            download.directory.as_deref(),
            Some("/home/test/Downloads/Example Torrent")
        );
    }

    #[test]
    fn preserves_base_directory_for_single_file_torrents() {
        let response: Aria2DownloadResponse = serde_json::from_value(serde_json::json!({
            "gid": "0123456789abcdef",
            "status": "active",
            "totalLength": "1024",
            "completedLength": "512",
            "downloadSpeed": "128",
            "dir": "/home/test/Downloads",
            "bittorrent": {
                "mode": "single",
                "info": {
                    "name": "Example.iso"
                }
            },
            "files": [
                {
                    "path": "/home/test/Downloads/Example.iso",
                    "length": "1024",
                    "completedLength": "512",
                    "uris": []
                }
            ]
        }))
        .unwrap();

        let download = response.into_download().unwrap();

        assert_eq!(download.name, "Example.iso");
        assert_eq!(download.directory.as_deref(), Some("/home/test/Downloads"));
    }
}
