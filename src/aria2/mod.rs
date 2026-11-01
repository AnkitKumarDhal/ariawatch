// Handles aria2 JSON-RPC requests and responses
mod downloads;
mod operations;
mod rpc;
mod torrents;
pub use downloads::{
    Aria2DownloadResponse, build_tell_active_request, fetch_all_downloads, fetch_downloads,
};
pub use operations::{
    add_torrent_file, add_uri, get_download_file_paths, pause_all, pause_download, remove_download,
    resume_all, resume_download,
};
pub use rpc::{build_get_version_request, build_rpc_request, parse_rpc_response, send_rpc_request};
pub use torrents::{TorrentFile, get_torrent_files, set_torrent_file_selection};
