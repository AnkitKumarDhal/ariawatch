use crate::config::{DownloadsConfig, UserConfig};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const CONFIG_DIR_MODE: u32 = 0o700;
const CONFIG_FILE_MODE: u32 = 0o600;

// this loads the config file, and creates a default file if one does not exist, that too with default settings (pretty neat feature imo)
pub fn load_or_create(path: &Path) -> Result<UserConfig, String> {
    let parent = parent_dir(path)?;
    prepare_config_dir(parent)?;

    match fs::symlink_metadata(path) {
        Ok(_) => read_config(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let config = UserConfig {
                downloads: DownloadsConfig::default(),
            };

            let contents = toml::to_string_pretty(&config)
                .map_err(|error| format!("Could not serialize default config: {error}"))?;

            if create_default_file(path, &contents)? {
                Ok(config)
            } else {
                read_config(path)
            }
        }

        Err(error) => Err(format!(
            "Could not inspect config file {}: {error}",
            path.display()
        )),
    }
}

// this one serializes (writes it into temp) and *atomically* replaces it with the existing file (so
// that there are no broken files that are a result of the operation being somehow, by some godzilla
// attack, being stopped midway)
pub fn save(path: &Path, config: &UserConfig) -> Result<(), String> {
    let parent = parent_dir(path)?;
    prepare_config_dir(parent)?;
    validate_existing_file(path)?;

    let contents = toml::to_string_pretty(config)
        .map_err(|error| format!("Could not serialize config: {error}"))?;

    let temporary_path = write_temporary_file(path, &contents)?;

    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!(
            "Could not save config file {}: {error}",
            path.display()
        ));
    }

    Ok(())
}

// this just extracts the parent directory
fn parent_dir(path: &Path) -> Result<&Path, String> {
    if path.file_name().is_none() {
        return Err(String::from("Invalid configuration file pathj"));
    }

    Ok(path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new(".")))
}

// this one creates the config directory, verifies all that shi about it (type, ownership, permissions)
fn prepare_config_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| {
        format!(
            "Could not create configuration directory {}: {error}",
            path.display()
        )
    })?;

    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "Could not inspect configuration directory {}: {error}",
            path.display()
        )
    })?;

    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "Configuration path must be a real directory: {}",
            path.display()
        ));
    }

    let current_uid = unsafe { libc::geteuid() };

    if metadata.uid() != current_uid {
        return Err(format!(
            "Configuration directory is not owned by the current user: {}",
            path.display()
        ));
    }

    fs::set_permissions(path, fs::Permissions::from_mode(CONFIG_DIR_MODE)).map_err(|error| {
        format!(
            "Could not secure configuration directory {}: {error}",
            path.display()
        )
    })
}

// checks whether a file is a regular file owned by the current user, is not a symlink and has no
// additional hard links
fn validate_file_metadata(path: &Path, metadata: &fs::Metadata) -> Result<(), String> {
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "Configuration path must not be a symlink: {}",
            path.display()
        ));
    }

    if !metadata.is_file() {
        return Err(format!(
            "Configuration path is not a regular file: {}",
            path.display()
        ));
    }

    let current_uid = unsafe { libc::geteuid() };

    if metadata.uid() != current_uid {
        return Err(format!(
            "Configuration directory is not owned by the current user: {}",
            path.display()
        ));
    }

    if metadata.nlink() != 1 {
        return Err(format!(
            "Configuration file must not have multiple hard link: {}",
            path.display()
        ));
    }

    Ok(())
}

// checks whether an existing config file is safe (to use)
fn validate_existing_file(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => validate_file_metadata(path, &metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Could not inspect configuration file {}: {error}",
            path.display()
        )),
    }
}

// *safely* opens, validates, reads, and extracts the toml config
fn read_config(path: &Path) -> Result<UserConfig, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "Could not inspect configuration file {}: {error}",
            path.display()
        )
    })?;

    validate_file_metadata(path, &metadata)?;

    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| format!("Could not open config file {}: {error}", path.display()))?;

    let metadata = file.metadata().map_err(|error| {
        format!(
            "Could not inspect opened config file {}: {error}",
            path.display()
        )
    })?;

    validate_file_metadata(path, &metadata)?;

    file.set_permissions(fs::Permissions::from_mode(CONFIG_FILE_MODE))
        .map_err(|error| format!("Could not secure config file: {error}"))?;

    let mut contents = String::new();

    file.read_to_string(&mut contents)
        .map_err(|error| format!("Could not read config file {}: {error}", path.display()))?;

    toml::from_str(&contents)
        .map_err(|error| format!("Invalid config in {}: {error}", path.display()))
}

// creates the initial default config without overwriting the file created *concurrently* by another process
fn create_default_file(path: &Path, contents: &str) -> Result<bool, String> {
    let temporary_path = write_temporary_file(path, contents)?;
    let result = fs::hard_link(&temporary_path, path);
    let cleanup = fs::remove_file(&temporary_path);

    if let Err(error) = cleanup {
        return Err(format!(
            "Could not remove temporary config file {}: {error}",
            temporary_path.display()
        ));
    }

    match result {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),

        Err(error) => Err(format!(
            "Could not create config file {}: {error}",
            path.display()
        )),
    }
}

// writes config contents to a uniquely named temp file with restrictive permissions
fn write_temporary_file(path: &Path, contents: &str) -> Result<PathBuf, String> {
    let parent = parent_dir(path)?;
    let file_name = path.file_name().unwrap().to_string_lossy();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let temporary_path = parent.join(format!(
        ".{file_name}.{}.{timestamp}.tmp",
        std::process::id()
    ));

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(CONFIG_FILE_MODE)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(&temporary_path)
        .map_err(|error| {
            format!(
                "Could not create temporary config file {}: {error}",
                temporary_path.display()
            )
        })?;

    let result = file
        .write_all(contents.as_bytes())
        .and_then(|()| file.sync_all());

    if let Err(error) = result {
        drop(file);
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("COuld not write temporary config file: {error}"));
    }

    drop(file);

    Ok(temporary_path)
}

#[cfg(test)]
mod tests {
    use super::{load_or_create, save};
    use crate::config::{DownloadsConfig, UserConfig};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn test_path() -> (PathBuf, PathBuf) {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let root = std::env::temp_dir().join(format!(
            "ariawatch-config-test-{}-{unique_id}",
            std::process::id()
        ));

        fs::create_dir(&root).unwrap();

        let config_path = root.join("ariawatch").join("config.toml");

        (root, config_path)
    }

    #[test]
    fn creates_default_configuration_when_file_is_missing() {
        let (root, path) = test_path();

        let config = load_or_create(&path).unwrap();

        assert_eq!(config, UserConfig::default());
        assert!(path.is_file());

        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[downloads]"));
        assert!(contents.contains("directory = \"Downloads\""));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loads_existing_configuration() {
        let (root, path) = test_path();

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "[downloads]\ndirectory = \"Documents/Downloads\"\n").unwrap();

        let config = load_or_create(&path).unwrap();

        assert_eq!(
            config.downloads.directory,
            PathBuf::from("Documents/Downloads")
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_directory_field_uses_default() {
        let (root, path) = test_path();

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "[downloads]\n").unwrap();

        let config = load_or_create(&path).unwrap();

        assert_eq!(config.downloads, DownloadsConfig::default());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn saves_and_loads_configuration() {
        let (root, path) = test_path();

        let config = UserConfig {
            downloads: DownloadsConfig {
                directory: PathBuf::from("/mnt/storage/downloads"),
            },
        };

        save(&path, &config).unwrap();

        let loaded = load_or_create(&path).unwrap();

        assert_eq!(loaded, config);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_configuration_is_not_overwritten() {
        let (root, path) = test_path();

        fs::create_dir_all(path.parent().unwrap()).unwrap();

        let original = "this is not valid TOML = [";
        fs::write(&path, original).unwrap();

        assert!(load_or_create(&path).is_err());

        assert_eq!(fs::read_to_string(&path).unwrap(), original);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unknown_toml_fields_are_ignored() {
        let (root, path) = test_path();

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "[unknown]\nenabled = true\n\
             [downloads]\ndirectory = \"Documents\"\nunknown_option = 42\n",
        )
        .unwrap();

        let config = load_or_create(&path).unwrap();

        assert_eq!(config.downloads.directory, PathBuf::from("Documents"));

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn creates_private_configuration_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let (root, path) = test_path();

        load_or_create(&path).unwrap();

        let directory_mode = fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;

        let file_mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;

        assert_eq!(directory_mode, 0o700);
        assert_eq!(file_mode, 0o600);

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_configuration_file_symlinks() {
        use std::os::unix::fs::symlink;

        let (root, path) = test_path();

        fs::create_dir_all(path.parent().unwrap()).unwrap();

        let target = root.join("target.toml");
        fs::write(&target, "[downloads]\ndirectory = \"Downloads\"\n").unwrap();

        symlink(&target, &path).unwrap();

        assert!(load_or_create(&path).is_err());

        // The symlink target must remain untouched.
        assert!(target.is_file());

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_configuration_files_with_multiple_hard_links() {
        let (root, path) = test_path();

        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "[downloads]\ndirectory = \"Downloads\"\n").unwrap();

        let second_link = root.join("second-link.toml");
        fs::hard_link(&path, &second_link).unwrap();

        assert!(load_or_create(&path).is_err());

        fs::remove_dir_all(root).unwrap();
    }
}
