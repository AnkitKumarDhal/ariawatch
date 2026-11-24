use super::rpc::{build_rpc_request, call_rpc_method, parse_rpc_response, send_rpc_request};
use crate::domain::status::Status;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use std::{
    fs,
    path::{Path, PathBuf},
};

// normally RPC request allows a max of 2.0 MiB of transfer, so limiting it to 1.4MB is a good way
// to not hit these limits
const MAX_TORRENT_BYTES: u64 = 1_400_000;

// the actual dir and file paths reported by aria2 for *one* downlaod
#[derive(Debug, PartialEq, Eq)]
pub struct DownloadFileInfo {
    pub directory: PathBuf,
    pub paths: Vec<PathBuf>,
}

// validates a DD URI before submitting it to aria2 via the build_add_uri_request() function
pub fn validate_download_uri(uri: &str) -> Result<(), String> {
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

    Ok(())
}

// does the request for adding a download link (even supports magnets)
pub fn build_add_uri_request(uri: &str, rpc_secret: Option<&str>) -> Result<String, String> {
    let uri = uri.trim();
    validate_download_uri(uri)?;
    build_rpc_request("aria2.addUri", vec![serde_json::json!([uri])], rpc_secret)
}

// this function is kinda critical on timing ya' know? this one builds an aria2.addUri request (like
// hte previous one) and then pauses it right after retrieving the magnet metadata
pub(super) fn build_add_magnet_request(
    uri: &str,
    rpc_secret: Option<&str>,
) -> Result<String, String> {
    let uri = uri.trim();

    if !uri.to_ascii_lowercase().starts_with("magnet:?") {
        return Err(String::from("Expected a magnet URI"));
    }

    if uri.is_empty() || uri.chars().any(char::is_whitespace) {
        return Err(String::from("Magnet URI is empty or contains whitespace"));
    }

    build_rpc_request(
        "aria2.addUri",
        vec![
            serde_json::json!([uri]),
            serde_json::json!({"pause-metadata": "true"}),
        ],
        rpc_secret,
    )
}

// now this one, its similar (kinda confusing, now that i look at it), but the important distinction
// here is that it builds an *aria2.addTorrent* request taht pauses the torrent after its metadata
// has been *processed*
pub(super) fn build_add_paused_torrent_request(
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
        vec![
            serde_json::json!(encoded),
            serde_json::json!([]),
            serde_json::json!({"pause-metadata": "true", "pause": "true"}),
        ],
        rpc_secret,
    )
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

// this one adds a magnet uri and pauses the torrent that comes after fetching the metadata
pub fn add_magnet_for_selection(
    endpoint: &str,
    uri: &str,
    rpc_secret: Option<&str>,
) -> Result<String, String> {
    let request = build_add_magnet_request(uri, rpc_secret)?;
    let body = send_rpc_request(endpoint, &request)?;
    let response = parse_rpc_response(&body)?;
    let result = response.into_result("ariawatch-1")?;

    expect_gid(result, "aria2.addUri")
}

// this one adds a local torrent file for file selection, in a paused state
pub fn add_paused_torrent_file(
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
        return Err(String::from("Torrent path is not a regular file "));
    }

    if metadata.len() == 0 || metadata.len() > MAX_TORRENT_BYTES {
        return Err(String::from(
            "Torrent file must be non-empty and no larger than 1.4MB",
        ));
    }

    let contents = fs::read(path)
        .map_err(|error| format!("Could not read torrent file {}: {error}", path.display()))?;

    let request = build_add_paused_torrent_request(&contents, rpc_secret)?;
    let body = send_rpc_request(endpoint, &request)?;
    let response = parse_rpc_response(&body)?;
    let result = response.into_result("ariawatch-1")?;

    expect_gid(result, "aria2.addTorrent")
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
pub(super) fn validate_gid(gid: &str) -> Result<(), String> {
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
    let result = match call_rpc_method(
        endpoint,
        "aria2.removeDownloadResult",
        vec![serde_json::json!(gid)],
        rpc_secret,
    ) {
        Ok(result) => result,
        Err(error) if is_missing_download_result_error(&error, gid) => {
            return Ok(());
        }
        Err(error) => return Err(error),
    };

    if result.as_str() == Some("OK") {
        Ok(())
    } else {
        Err(String::from(
            "aria2.removeDownloadResult returned an unexpected result",
        ))
    }
}

fn is_missing_download_result_error(error: &str, gid: &str) -> bool {
    error.contains(&format!("Could not remove download result of GID#{gid}"))
        || error.contains(&format!("GID {gid} is not found"))
        || error.contains(&format!("GID#{gid} is not found"))
}

pub fn remove_download(
    endpoint: &str,
    gid: &str,
    status: &Status,
    rpc_secret: Option<&str>,
) -> Result<(), String> {
    validate_gid(gid)?;

    match status {
        Status::Active | Status::Seeding | Status::Waiting | Status::Paused => {
            let result = call_rpc_method(
                endpoint,
                "aria2.remove",
                vec![serde_json::json!(gid)],
                rpc_secret,
            )
            .map_err(|error| format!("aria2.remove failed for GID {gid}: {error}"))?;

            if result.as_str() != Some(gid) {
                return Err(String::from("aria2.remove returned an unexpected result"));
            }

            remove_download_result(endpoint, gid, rpc_secret)
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

        // if there were files in the torrent downlaod that were specfically set to not be
        // downloaded when starting the torrent download, this part makes sure that those
        // "unselected" files are not indexed for deleting
        let is_unselected = match object.get("selected") {
            Some(serde_json::Value::String(selected)) => selected == "false",
            Some(serde_json::Value::Bool(selected)) => !selected,
            _ => false,
        };

        if is_unselected {
            continue;
        }

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

// extracts the download dir and selected file paths from aria2.tellStatus
fn parse_download_file_info(
    value: &serde_json::Value,
    source: &str,
) -> Result<DownloadFileInfo, String> {
    let dir = value
        .get("dir")
        .and_then(serde_json::Value::as_str)
        .filter(|directory| !directory.trim().is_empty())
        .ok_or_else(|| format!("{source} returned an invalid or missing `dir` field"))?;

    let dir = PathBuf::from(dir);

    if !dir.is_absolute() {
        return Err(format!(
            "{source} returned a non-absolute download directory: {}",
            dir.display()
        ));
    }

    let files = value
        .get("files")
        .ok_or_else(|| format!("{source} returned no `files` field"))?;

    let paths = parse_download_file_paths(files, &format!("{source}.files"))?;

    Ok(DownloadFileInfo {
        directory: dir,
        paths,
    })
}

// figures out the file path to be deleted
pub fn get_download_file_info(
    endpoint: &str,
    gid: &str,
    rpc_secret: Option<&str>,
) -> Result<DownloadFileInfo, String> {
    validate_gid(gid)?;

    let status = call_rpc_method(
        endpoint,
        "aria2.tellStatus",
        vec![serde_json::json!(gid), serde_json::json!(["dir", "files"])],
        rpc_secret,
    )
    .map_err(|error| format!("aria2.tellStatus failed for GID {gid}: {error}"))?;

    parse_download_file_info(&status, "aria2.tellStatus")
}

#[cfg(test)]
mod control_tests {
    use crate::aria2::operations::parse_download_file_info;

    use super::{
        build_add_torrent_request, build_add_uri_request, is_missing_download_result_error,
        parse_download_file_paths, validate_gid,
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

    #[test]
    fn builds_magnet_request_with_metadata_pause() {
        let request =
            super::build_add_magnet_request("magnet:?xt=urn:btih:0123456789abcdef", None).unwrap();

        let value: serde_json::Value = serde_json::from_str(&request).unwrap();

        assert_eq!(value["method"], "aria2.addUri");
        assert_eq!(
            value["params"][0][0],
            "magnet:?xt=urn:btih:0123456789abcdef"
        );
        assert_eq!(value["params"][1]["pause-metadata"], "true");
    }

    #[test]
    fn builds_local_torrent_request_with_pause_options() {
        let request =
            super::build_add_paused_torrent_request(b"fake torrent contents", None).unwrap();

        let value: serde_json::Value = serde_json::from_str(&request).unwrap();

        assert_eq!(value["method"], "aria2.addTorrent");
        assert_eq!(value["params"][1], serde_json::json!([]));
        assert_eq!(value["params"][2]["pause-metadata"], "true");
        assert_eq!(value["params"][2]["pause"], "true");
    }

    #[test]
    fn rejects_non_magnet_uri_for_metadata_request() {
        assert!(super::build_add_magnet_request("https://example.com/file.zip", None).is_err());
    }

    #[test]
    fn recognizes_an_already_absent_download_result() {
        let gid = "ff30d62a336365b0";
        let error = format!("aria2 rpc error 1: Could not remove download result of GID#{gid}");

        assert!(is_missing_download_result_error(&error, gid));
    }

    #[test]
    fn does_not_ignore_unrelated_removal_errors() {
        let error = "aria2 rpc error 1: Invalid GID";

        assert!(!is_missing_download_result_error(error, "ff30d62a336365b0"));
    }

    #[test]
    fn parses_torrent_download_directory_and_selected_nested_files() {
        let directory = std::env::temp_dir().join("ariawatch-test-downloads");

        let selected_path = directory
            .join("Example Torrent")
            .join("Disc 1")
            .join("video.mkv");

        let unselected_path = directory
            .join("Example Torrent")
            .join("Disc 2")
            .join("video.mkv");

        let status = serde_json::json!({
            "dir": directory.to_string_lossy(),
            "files": [
                {
                    "index": "1",
                    "path": selected_path.to_string_lossy(),
                    "selected": "true"
                },
                {
                    "index": "2",
                    "path": unselected_path.to_string_lossy(),
                    "selected": "false"
                }
            ]
        });

        let info = parse_download_file_info(&status, "aria2.tellStatus").unwrap();

        assert_eq!(info.directory, directory);
        assert_eq!(info.paths, vec![selected_path]);
    }

    #[test]
    fn recognizes_generic_missing_download_result_error() {
        let gid = "217badcbb15dd4ef";
        let error = format!("aria2 rpc error 1: GID {gid} is not found");

        assert!(is_missing_download_result_error(&error, gid));
    }
}
