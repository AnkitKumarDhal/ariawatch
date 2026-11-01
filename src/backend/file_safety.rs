use std::{
    fs,
    path::{Path, PathBuf},
};

// managed controller starts aria2 with $HOME/Downloads as its directory
pub(super) fn managed_download_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| String::from("Could not determine the home directory"))?;

    let directory = home.join("Downloads");

    let canonical = fs::canonicalize(&directory).map_err(|error| {
        format!(
            "Could not resolve managed download directory {}: {error}",
            directory.display()
        )
    })?;

    if !canonical.is_dir() {
        return Err(String::from(
            "Managed downlaod directory is not a directory",
        ));
    }

    Ok(canonical)
}

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
