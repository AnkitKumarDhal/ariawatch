use crate::{aria2, domain::download::Download};
use std::collections::HashSet;

// Downlods the full list and converts it into our internal model
pub(super) fn fetch_snapshot(
    endpoint: &str,
    rpc_secret: Option<&str>,
) -> Result<Vec<Download>, String> {
    let (_, _, _, responses) = aria2::fetch_all_downloads(endpoint, rpc_secret)?;

    responses
        .into_iter()
        .map(|response| response.into_download())
        .collect()
}

pub(super) fn fetch_active(
    endpoint: &str,
    rpc_secret: Option<&str>,
) -> Result<Vec<Download>, String> {
    let request = aria2::build_tell_active_request(rpc_secret)?;

    aria2::fetch_downloads(endpoint, &request)?
        .into_iter()
        .map(|response| response.into_download())
        .collect()
}

pub(super) fn active_gid_set(downloads: &[Download]) -> HashSet<String> {
    downloads
        .iter()
        .filter(|download| download.status.is_active())
        .map(|download| download.gid.clone())
        .collect()
}
