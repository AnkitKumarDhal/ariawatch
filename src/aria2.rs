// Handles aria2 JSON-RPC requests and responses
mod downloads;
mod operations;
mod rpc;
use crate::domain::{download::Download, status::Status};
pub use downloads::{
    Aria2DownloadResponse, Aria2FileResponse, Aria2UriResponse, build_tell_active_request,
    build_tell_stopped_request, build_tell_waiting_request, fetch_all_downloads, fetch_downloads,
    parse_download_list,
};
pub use operations::{
    add_torrent_file, add_uri, get_download_file_paths, pause_all, pause_download, remove_download,
    resume_all, resume_download,
};
pub use rpc::{build_get_version_request, build_rpc_request, parse_rpc_response, send_rpc_request};
use serde::Deserialize;
use std::{collections::HashMap, path::Path};
