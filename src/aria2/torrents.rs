use crate::aria2::{operations::validate_gid, rpc::call_rpc_method};
use serde::Deserialize;
use std::collections::HashSet;

// this is the metadata style of one file of a torrent
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorrentFile {
    pub index: u32,
    pub path: String,
    pub length: u64,
    pub completed_length: u64,
    pub selected: bool,
}

// this is the response style provided by aria2.getFiles
#[derive(Debug, Deserialize)]
struct RawTorrentFile {
    index: String,
    path: String,
    length: String,

    #[serde(rename = "completedLength")]
    completed_length: String,

    selected: String,
}

impl TryFrom<RawTorrentFile> for TorrentFile {
    type Error = String;

    fn try_from(raw: RawTorrentFile) -> Result<Self, Self::Error> {
        let index = raw
            .index
            .parse::<u32>()
            .map_err(|error| format!("Invalid torrent file index '{}': {error}", raw.index))?;

        if index == 0 {
            return Err(String::from(
                "Invalid torrent file index: aria2 indices start at 1",
            ));
        }

        let length = raw
            .length
            .parse::<u64>()
            .map_err(|error| format!("Invalid length for torrent file {index}: {error}"))?;

        let completed_length = raw.completed_length.parse::<u64>().map_err(|error| {
            format!("Invalid completed length for torrent file {index}: {error}")
        })?;

        let selected = raw.selected.parse::<bool>().map_err(|error| {
            format!("Invalid selection state for torrent file {index}: {error}")
        })?;

        Ok(Self {
            index,
            path: raw.path,
            length,
            completed_length,
            selected,
        })
    }
}

// parses the thingy given by aria2.getFiles
fn parse_torrent_files(value: serde_json::Value) -> Result<Vec<TorrentFile>, String> {
    let entries = value
        .as_array()
        .ok_or_else(|| String::from("aria2.getFiles returned an invalid file list"))?;

    let mut files = Vec::with_capacity(entries.len());
    let mut seen_indices = HashSet::with_capacity(entries.len());

    for (position, entry) in entries.iter().enumerate() {
        let raw = serde_json::from_value::<RawTorrentFile>(entry.clone()).map_err(|error| {
            format!(
                "Could not parse aria2 torrent file entry {}: {error}",
                position + 1
            )
        })?;

        let file = TorrentFile::try_from(raw)?;

        if !seen_indices.insert(file.index) {
            return Err(format!(
                "aria2 returned a duplicate torrent file index: {}",
                file.index
            ));
        }

        files.push(file);
    }

    Ok(files)
}

// fetches the file list for a torrent/magnet
pub fn get_torrent_files(
    endpoint: &str,
    gid: &str,
    rpc_secret: Option<&str>,
) -> Result<Vec<TorrentFile>, String> {
    validate_gid(gid)?;

    let result = call_rpc_method(
        endpoint,
        "aria2.getFiles",
        vec![serde_json::json!(gid)],
        rpc_secret,
    )?;

    parse_torrent_files(result)
}

// this one builds the comma-separated value expected by aria2's select-file option
fn build_select_file_value(indices: &[u32]) -> Result<String, String> {
    if indices.is_empty() {
        return Err(String::from("At least one torrent file must be selected"));
    }

    let mut seen = HashSet::with_capacity(indices.len());
    let mut values = Vec::with_capacity(indices.len());

    for index in indices {
        if *index == 0 {
            return Err(String::from(
                "Invalid torrent file index: aria2 indices start at 1",
            ));
        }

        if !seen.insert(*index) {
            return Err(format!("Duplicate torrent file selection index: {index}"));
        }

        values.push(index.to_string());
    }

    Ok(values.join(","))
}

// Applies the selected file indices to a torrent
pub fn set_torrent_file_selection(
    endpoint: &str,
    gid: &str,
    indices: &[u32],
    rpc_secret: Option<&str>,
) -> Result<(), String> {
    validate_gid(gid)?;

    let selection = build_select_file_value(indices)?;
    let result = call_rpc_method(
        endpoint,
        "aria2.changeOption",
        vec![
            serde_json::json!(gid),
            serde_json::json!({"select-file": selection}),
        ],
        rpc_secret,
    )?;

    match result.as_str() {
        Some("OK") => Ok(()),
        _ => Err(String::from(
            "aria2.changeOption returned an unexpected result",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{TorrentFile, build_select_file_value, parse_torrent_files};

    #[test]
    fn parses_torrent_file_metadata() {
        let value = serde_json::json!([
            {
                "index": "1",
                "path": "Example/file.txt",
                "length": "2048",
                "completedLength": "1024",
                "selected": "true"
            },
            {
                "index": "2",
                "path": "Example/image.png",
                "length": "4096",
                "completedLength": "0",
                "selected": "false"
            }
        ]);

        let files = parse_torrent_files(value).unwrap();

        assert_eq!(
            files,
            vec![
                TorrentFile {
                    index: 1,
                    path: String::from("Example/file.txt"),
                    length: 2048,
                    completed_length: 1024,
                    selected: true,
                },
                TorrentFile {
                    index: 2,
                    path: String::from("Example/image.png"),
                    length: 4096,
                    completed_length: 0,
                    selected: false,
                }
            ]
        );
    }

    #[test]
    fn accepts_an_empty_file_list() {
        let files = parse_torrent_files(serde_json::json!([])).unwrap();

        assert!(files.is_empty());
    }

    #[test]
    fn rejects_a_non_array_response() {
        let result = parse_torrent_files(serde_json::json!({
            "index": "1"
        }));

        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_numeric_fields() {
        let value = serde_json::json!([
            {
                "index": "one",
                "path": "file.txt",
                "length": "1024",
                "completedLength": "0",
                "selected": "true"
            }
        ]);

        assert!(parse_torrent_files(value).is_err());
    }

    #[test]
    fn rejects_zero_file_indices() {
        let value = serde_json::json!([
            {
                "index": "0",
                "path": "file.txt",
                "length": "1024",
                "completedLength": "0",
                "selected": "true"
            }
        ]);

        assert!(parse_torrent_files(value).is_err());
    }

    #[test]
    fn rejects_invalid_selection_values() {
        let value = serde_json::json!([
            {
                "index": "1",
                "path": "file.txt",
                "length": "1024",
                "completedLength": "0",
                "selected": "yes"
            }
        ]);

        assert!(parse_torrent_files(value).is_err());
    }

    #[test]
    fn rejects_duplicate_indices_in_response() {
        let value = serde_json::json!([
            {
                "index": "1",
                "path": "first.txt",
                "length": "1024",
                "completedLength": "0",
                "selected": "true"
            },
            {
                "index": "1",
                "path": "second.txt",
                "length": "2048",
                "completedLength": "0",
                "selected": "true"
            }
        ]);

        assert!(parse_torrent_files(value).is_err());
    }

    #[test]
    fn builds_selection_value() {
        assert_eq!(build_select_file_value(&[1, 3, 5]).unwrap(), "1,3,5");
    }

    #[test]
    fn accepts_a_single_selected_file() {
        assert_eq!(build_select_file_value(&[2]).unwrap(), "2");
    }

    #[test]
    fn rejects_an_empty_selection() {
        assert!(build_select_file_value(&[]).is_err());
    }

    #[test]
    fn rejects_zero_selection_indices() {
        assert!(build_select_file_value(&[0, 1]).is_err());
    }

    #[test]
    fn rejects_duplicate_selection_indices() {
        assert!(build_select_file_value(&[1, 2, 1]).is_err());
    }
}
