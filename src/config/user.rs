// this guy right here, it is the home for the user config handling
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UserConfig {
    pub downloads: DownloadsConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadsConfig {
    pub directory: PathBuf,
}

impl Default for DownloadsConfig {
    fn default() -> Self {
        Self {
            directory: PathBuf::from("Downloads"),
        }
    }
}

impl DownloadsConfig {
    // Resolves the configured directory according to the user's home directory
    // in the config file, relative directories are automatically expanded to the home
    // directory/specified directory
    // Absolute directories are preserved as they are
    pub fn resolve_directory(&self, home_dir: &Path) -> PathBuf {
        if self.directory.is_absolute() {
            return self.directory.clone();
        }

        if self.directory == Path::new("~") {
            return home_dir.to_path_buf();
        }

        if let Ok(relative_path) = self.directory.strip_prefix("~/") {
            return home_dir.join(relative_path);
        }

        home_dir.join(&self.directory)
    }
}

#[cfg(test)]
mod tests {
    use super::{DownloadsConfig, UserConfig};
    use std::path::PathBuf;

    #[test]
    fn default_configuration_uses_downloads_directory() {
        let config = UserConfig::default();

        assert_eq!(config.downloads.directory, PathBuf::from("Downloads"));
    }

    #[test]
    fn missing_downloads_section_uses_default_configuration() {
        let config: UserConfig = serde_json::from_str("{}").unwrap();

        assert_eq!(config, UserConfig::default());
    }

    #[test]
    fn missing_directory_field_uses_default_directory() {
        let config: UserConfig = serde_json::from_str(r#"{"downloads":{}}"#).unwrap();

        assert_eq!(config.downloads, DownloadsConfig::default());
    }

    #[test]
    fn resolves_relative_directory_against_home_directory() {
        let config = DownloadsConfig {
            directory: PathBuf::from("Documents/Downloads"),
        };

        let resolved = config.resolve_directory(&PathBuf::from("/home/test"));

        assert_eq!(resolved, PathBuf::from("/home/test/Documents/Downloads"));
    }

    #[test]
    fn preserves_absolute_download_directory() {
        let config = DownloadsConfig {
            directory: PathBuf::from("/mnt/storage/downloads"),
        };

        let resolved = config.resolve_directory(&PathBuf::from("/home/test"));

        assert_eq!(resolved, PathBuf::from("/mnt/storage/downloads"));
    }

    #[test]
    fn ignores_unknown_configuration_fields() {
        let config: UserConfig = serde_json::from_str(
            r#"
            {
                "unknown_setting": true,
                "downloads": {
                    "directory": "Documents",
                    "unknown_option": "ignored"
                }
            }
            "#,
        )
        .unwrap();

        assert_eq!(config.downloads.directory, PathBuf::from("Documents"));
    }

    #[test]
    fn resolves_home_directory_shorthand() {
        let config = DownloadsConfig {
            directory: PathBuf::from("~"),
        };

        let resolved = config.resolve_directory(&PathBuf::from("/home/test"));

        assert_eq!(resolved, PathBuf::from("/home/test"));
    }

    #[test]
    fn resolves_tilde_prefixed_download_directory() {
        let config = DownloadsConfig {
            directory: PathBuf::from("~/Downloads"),
        };

        let resolved = config.resolve_directory(&PathBuf::from("/home/test"));

        assert_eq!(resolved, PathBuf::from("/home/test/Downloads"));
    }

    #[test]
    fn does_not_expand_named_user_shorthand() {
        let config = DownloadsConfig {
            directory: PathBuf::from("~anotheruser/Downloads"),
        };

        let resolved = config.resolve_directory(&PathBuf::from("/home/test"));

        assert_eq!(resolved, PathBuf::from("/home/test/~anotheruser/Downloads"));
    }
}
