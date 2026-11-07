use std::{
    fs,
    path::{Path, PathBuf},
};

// validate the files before deleting them
pub(super) fn collect_safe_files(
    paths: Vec<PathBuf>,
    directory: &Path,
) -> Result<Vec<PathBuf>, String> {
    let canonical_directory = fs::canonicalize(directory).map_err(|error| {
        format!(
            "Could not resolve managed download directory {}: {error}",
            directory.display()
        )
    })?;

    let mut safe_files = Vec::new();

    for path in paths {
        let candidate = if path.is_absolute() {
            path
        } else {
            canonical_directory.join(path)
        };

        let metadata = match fs::symlink_metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "Could not inspect download file {}: {error}",
                    candidate.display()
                ));
            }
        };

        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Refusing to delete a symbolic link: {}",
                candidate.display()
            ));
        }

        if !metadata.is_file() {
            return Err(format!(
                "Refusing to delete a non-regular file: {}",
                candidate.display()
            ));
        }

        let canonical_file = fs::canonicalize(&candidate).map_err(|error| {
            format!(
                "Could not resolve download file {}: {error}",
                candidate.display()
            )
        })?;

        if canonical_file == canonical_directory
            || !canonical_file.starts_with(&canonical_directory)
        {
            return Err(format!(
                "Refusing to delete a file outside the managed download directory: {}",
                canonical_file.display()
            ));
        }

        safe_files.push(canonical_file);
    }

    safe_files.sort();
    safe_files.dedup();

    Ok(safe_files)
}

pub(super) fn remove_empty_parent_directories(
    files: &[PathBuf],
    directory: &Path,
) -> Result<usize, String> {
    let canonical_directory = fs::canonicalize(directory).map_err(|error| {
        format!(
            "Could not resolve managed download directory {}: {error}",
            directory.display()
        )
    })?;

    let mut removed_directories = 0;

    for file in files {
        if file == &canonical_directory || !file.starts_with(&canonical_directory) {
            return Err(format!(
                "Refusing to clean directories outside the managed download directory: {}",
                file.display()
            ));
        }

        let mut current = file.parent().map(Path::to_path_buf);

        while let Some(candidate) = current {
            if candidate == canonical_directory {
                break;
            }

            if !candidate.starts_with(&canonical_directory) {
                return Err(format!(
                    "Refusing to remove a directory outside the managed download directory: {}",
                    candidate.display()
                ));
            }

            let next_parent = candidate.parent().map(Path::to_path_buf);

            let metadata = match fs::symlink_metadata(&candidate) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    current = next_parent;
                    continue;
                }
                Err(error) => {
                    return Err(format!(
                        "Could not inspect download directory {}: {error}",
                        candidate.display()
                    ));
                }
            };

            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "Refusing to remove a symlink directory: {}",
                    candidate.display()
                ));
            }

            if !metadata.is_dir() {
                return Err(format!(
                    "Refusing to remove a non-directory path: {}",
                    candidate.display()
                ));
            }

            let canonical_candidate = match fs::canonicalize(&candidate) {
                Ok(path) => path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    current = next_parent;
                    continue;
                }
                Err(error) => {
                    return Err(format!(
                        "Could not resolve download directory {}: {error}",
                        candidate.display()
                    ));
                }
            };

            if canonical_candidate != candidate
                || !canonical_candidate.starts_with(&canonical_directory)
            {
                return Err(format!(
                    "Refusing to remove a directory outside the managed download directory: {}",
                    candidate.display()
                ));
            }

            match fs::remove_dir(&candidate) {
                Ok(()) => removed_directories += 1,
                Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "Could not remove empty download directory {}: {error}",
                        candidate.display()
                    ));
                }
            }

            current = next_parent;
        }
    }

    Ok(removed_directories)
}

#[cfg(test)]
mod tests {
    use super::{collect_safe_files, remove_empty_parent_directories};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn collects_nested_torrent_files_under_download_directory() {
        let temp = create_test_directory("nested-torrent");
        let downloads = temp.join("configured-downloads");

        let torrent_directory = downloads.join("Example Torrent");
        let nested_directory = torrent_directory.join("Disc 1");

        let first_file = torrent_directory.join("cover.jpg");
        let second_file = nested_directory.join("video.mkv");

        fs::create_dir_all(&nested_directory).unwrap();
        fs::write(&first_file, b"cover").unwrap();
        fs::write(&second_file, b"video").unwrap();

        let safe_files =
            collect_safe_files(vec![first_file.clone(), second_file.clone()], &downloads).unwrap();

        assert_eq!(
            safe_files,
            vec![
                fs::canonicalize(&first_file).unwrap(),
                fs::canonicalize(&second_file).unwrap(),
            ]
        );

        fs::remove_dir_all(temp).unwrap();
    }

    fn create_test_directory(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let directory = std::env::temp_dir().join(format!(
            "ariawatch-{name}-{}-{timestamp}",
            std::process::id()
        ));

        fs::create_dir_all(&directory).unwrap();

        directory
    }

    #[test]
    fn removes_empty_torrent_directories_but_preserves_download_root() {
        let temp = create_test_directory("cleanup");
        let downloads = temp.join("Downloads");

        let torrent_directory = downloads.join("ExampleTorrent");
        let nested_directory = torrent_directory.join("Disc 1");
        let file = nested_directory.join("video.mkv");

        fs::create_dir_all(&nested_directory).unwrap();
        fs::write(&file, b"test video").unwrap();

        let canonical_file = fs::canonicalize(&file).unwrap();

        fs::remove_file(&canonical_file).unwrap();

        let removed = remove_empty_parent_directories(&[canonical_file], &downloads).unwrap();

        assert_eq!(removed, 2);
        assert!(downloads.is_dir());
        assert!(!torrent_directory.exists());

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn preserves_directories_containing_other_files() {
        let temp = create_test_directory("preserve");
        let downloads = temp.join("Downloads");

        let torrent_directory = downloads.join("ExampleTorrent");
        let file = torrent_directory.join("video.mkv");
        let other_file = torrent_directory.join("keep.txt");

        fs::create_dir_all(&torrent_directory).unwrap();
        fs::write(&file, b"test video").unwrap();
        fs::write(&other_file, b"keep this file").unwrap();

        let canonical_file = fs::canonicalize(&file).unwrap();

        fs::remove_file(&canonical_file).unwrap();

        let removed = remove_empty_parent_directories(&[canonical_file], &downloads).unwrap();

        assert_eq!(removed, 0);
        assert!(torrent_directory.is_dir());
        assert!(other_file.is_file());

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn rejects_paths_outside_managed_download_directory() {
        let temp = create_test_directory("outside");
        let downloads = temp.join("Downloads");
        let outside_file = temp.join("outside.txt");

        fs::create_dir_all(&downloads).unwrap();
        fs::write(&outside_file, b"do not delete").unwrap();

        let error = remove_empty_parent_directories(&[outside_file], &downloads).unwrap_err();

        assert!(error.contains("outside the managed download directory"));

        fs::remove_dir_all(temp).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_remove_symbolic_link_directories() {
        use std::os::unix::fs::symlink;

        let temp = create_test_directory("symlink");
        let downloads = temp.join("Downloads");
        let outside = temp.join("outside");
        let link = downloads.join("linked");

        fs::create_dir_all(&downloads).unwrap();
        fs::create_dir_all(&outside).unwrap();

        symlink(&outside, &link).unwrap();

        let fake_file = link.join("file.bin");

        let error = remove_empty_parent_directories(&[fake_file], &downloads).unwrap_err();

        assert!(error.contains("symlink directory"));

        fs::remove_dir_all(temp).unwrap();
    }
}
