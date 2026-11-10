// Handles aria2 JSON-RPC requests and responses
mod downloads;
mod operations;
mod rpc;
mod torrents;
pub use downloads::{
    Aria2DownloadResponse, build_tell_active_request, fetch_all_downloads, fetch_downloads,
};
pub use operations::{
    DownloadFileInfo, add_magnet_for_selection, add_paused_torrent_file, add_torrent_file, add_uri,
    get_download_file_info, pause_all, pause_download, remove_download, resume_all,
    resume_download, validate_download_uri,
};
pub use rpc::{build_get_version_request, parse_rpc_response, send_rpc_request};
pub use torrents::{
    TorrentFile, TorrentMetadataStatus, get_torrent_files, get_torrent_metadata_status,
    set_torrent_file_selection,
};
