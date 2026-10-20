// this file is the epitome of directory/path handling
use std::{env, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub runtime_dir: PathBuf,
}

impl AppPaths {
    // automatically sniffs out the paths
    pub fn discover() -> Result<Self, String> {
        Self::from_values(
            env::var_os("HOME").map(PathBuf::from),
            env::var_os("XDG_DATA_HOME").map(PathBuf::from),
            env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            env::var_os("XDG_STATE_HOME").map(PathBuf::from),
            env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        )
    }

    // *helps* with the automatic sniffing of paths
    fn from_values(
        home: Option<PathBuf>,
        data_home: Option<PathBuf>,
        config_home: Option<PathBuf>,
        state_home: Option<PathBuf>,
        runtime_home: Option<PathBuf>,
    ) -> Result<Self, String> {
        let home = home.filter(|path| path.is_absolute());

        let resolve_base = |provided: Option<PathBuf>,
                            fallback: &str,
                            variable: &str|
         -> Result<PathBuf, String> {
            if let Some(path) = provided.filter(|path| path.is_absolute()) {
                return Ok(path);
            }

            home.as_ref()
                .map(|home| home.join(fallback))
                .ok_or_else(|| {
                    format!(
                        "Could not determine {variable}: set it to an absolute path or set HOME"
                    )
                })
        };

        let data_home = resolve_base(data_home, ".local/share", "XDG_DATA_HOME")?;
        let config_home = resolve_base(config_home, ".config", "XDG_CONFIG_HOME")?;
        let state_home = resolve_base(state_home, ".local/state", "XDG_STATE_HOME")?;

        let runtime_dir = match runtime_home.filter(|path| path.is_absolute()) {
            Some(path) => path.join("ariawatch"),
            None => home
                .as_ref()
                .map(|home| home.join(".cache/ariawatch/runtime"))
                .ok_or_else(|| {
                    String::from(
                        "Could not determine runtime directory: set XDG_RUNTIME_DIR or HOME",
                    )
                })?,
        };

        Ok(Self {
            data_dir: data_home.join("ariawatch"),
            config_dir: config_home.join("ariawatch"),
            state_dir: state_home.join("ariawatch"),
            runtime_dir,
        })
    }

    // now comes the persistence part (kinda excited)
    // I'll explain each one (i need those explanations for myself)
    //
    // this one makes the actual persistent history file (in sqlite)
    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join("history.sqlite3")
    }

    // this one is just the aria2 session file
    pub fn aria2_session_file(&self) -> PathBuf {
        self.data_dir.join("aria2.session")
    }

    // this one is for the socket controller
    pub fn controller_socket_file(&self) -> PathBuf {
        self.runtime_dir.join("controller.sock")
    }

    // this one is for the controller pid value
    pub fn controller_pid_file(&self) -> PathBuf {
        self.runtime_dir.join("controller.pid")
    }

    // this one is for the ocntroller lock
    pub fn controller_lock_file(&self) -> PathBuf {
        self.runtime_dir.join("controller.lock")
    }

    // now it comes time for the actual directory making
    #[cfg(unix)]
    pub fn prepare_runtime_dir(&self) -> Result<(), String> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        match std::fs::symlink_metadata(&self.runtime_dir) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "Runtime directory must not be a symbolic link: {}",
                    self.runtime_dir.display()
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(format!(
                    "Runtime path is not a directory: {}",
                    self.runtime_dir.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir_all(&self.runtime_dir).map_err(|error| {
                    format!(
                        "Could not create runtime directory {}: {error}",
                        self.runtime_dir.display()
                    )
                })?;
            }
            Err(error) => {
                return Err(format!(
                    "Could not inspect runtime directory {}: {error}",
                    self.runtime_dir.display()
                ));
            }
        }

        let metadata = std::fs::symlink_metadata(&self.runtime_dir).map_err(|error| {
            format!(
                "COuld not inspect runtime directory {}: {error}",
                self.runtime_dir.display()
            )
        })?;

        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Runtime directory must not be a symbolic link: {}",
                self.runtime_dir.display()
            ));
        }

        if !metadata.is_dir() {
            return Err(format!(
                "Runtime path is not a directory: {}",
                self.runtime_dir.display()
            ));
        }

        let current_uid = unsafe { libc::geteuid() };

        if metadata.uid() != current_uid {
            return Err(format!(
                "Runtime directory is not owned by the current user: {}",
                self.runtime_dir.display()
            ));
        }

        std::fs::set_permissions(&self.runtime_dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| {
                format!(
                    "Could not secure runtime directory {}: {error}",
                    self.runtime_dir.display()
                )
            })?;

        Ok(())
    }

    #[cfg(not(unix))]
    pub fn prepare_runtime_dir(&self) -> Result<(), String> {
        Err(String::from(
            "Secure runtime directory preparation is currently supported only on Unix",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::AppPaths;
    use std::path::PathBuf;

    #[test]
    fn uses_home_fallbacks_when_xdg_directories_are_unset() {
        let paths =
            AppPaths::from_values(Some(PathBuf::from("/home/test")), None, None, None, None)
                .unwrap();

        assert_eq!(
            paths.data_dir,
            PathBuf::from("/home/test/.local/share/ariawatch")
        );
        assert_eq!(
            paths.config_dir,
            PathBuf::from("/home/test/.config/ariawatch")
        );
        assert_eq!(
            paths.state_dir,
            PathBuf::from("/home/test/.local/state/ariawatch")
        );
        assert_eq!(
            paths.runtime_dir,
            PathBuf::from("/home/test/.cache/ariawatch/runtime")
        );
    }

    #[test]
    fn respects_absolute_xdg_directories() {
        let paths = AppPaths::from_values(
            Some(PathBuf::from("/home/test")),
            Some(PathBuf::from("/data")),
            Some(PathBuf::from("/config")),
            Some(PathBuf::from("/state")),
            Some(PathBuf::from("/run/user/1000")),
        )
        .unwrap();

        assert_eq!(paths.data_dir, PathBuf::from("/data/ariawatch"));
        assert_eq!(paths.config_dir, PathBuf::from("/config/ariawatch"));
        assert_eq!(paths.state_dir, PathBuf::from("/state/ariawatch"));
        assert_eq!(paths.runtime_dir, PathBuf::from("/run/user/1000/ariawatch"));
    }

    #[test]
    fn ignores_relative_xdg_directory_values() {
        let paths = AppPaths::from_values(
            Some(PathBuf::from("/home/test")),
            Some(PathBuf::from("relative-data")),
            Some(PathBuf::from("relative-config")),
            Some(PathBuf::from("relative-state")),
            Some(PathBuf::from("relative-runtime")),
        )
        .unwrap();

        assert_eq!(
            paths.data_dir,
            PathBuf::from("/home/test/.local/share/ariawatch")
        );
        assert_eq!(
            paths.config_dir,
            PathBuf::from("/home/test/.config/ariawatch")
        );
        assert_eq!(
            paths.state_dir,
            PathBuf::from("/home/test/.local/state/ariawatch")
        );
        assert_eq!(
            paths.runtime_dir,
            PathBuf::from("/home/test/.cache/ariawatch/runtime")
        );
    }

    #[test]
    fn resolves_controller_artifact_paths() {
        let paths =
            AppPaths::from_values(Some(PathBuf::from("/home/test")), None, None, None, None)
                .unwrap();

        assert_eq!(
            paths.database_file(),
            PathBuf::from("/home/test/.local/share/ariawatch/history.sqlite3")
        );
        assert_eq!(
            paths.aria2_session_file(),
            PathBuf::from("/home/test/.local/share/ariawatch/aria2.session")
        );
        assert_eq!(
            paths.controller_socket_file(),
            PathBuf::from("/home/test/.cache/ariawatch/runtime/controller.sock")
        );
        assert_eq!(
            paths.controller_pid_file(),
            PathBuf::from("/home/test/.cache/ariawatch/runtime/controller.pid")
        );
        assert_eq!(
            paths.controller_lock_file(),
            PathBuf::from("/home/test/.cache/ariawatch/runtime/controller.lock")
        );
    }

    #[cfg(unix)]
    #[test]
    fn creates_runtime_directory_with_private_permissions() {
        use std::{
            fs,
            os::unix::fs::PermissionsExt,
            time::{SystemTime, UNIX_EPOCH},
        };

        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let root = std::env::temp_dir().join(format!(
            "ariawatch-runtime-test-{}-{unique_id}",
            std::process::id()
        ));

        let paths = AppPaths {
            data_dir: root.join("data"),
            config_dir: root.join("config"),
            state_dir: root.join("state"),
            runtime_dir: root.join("runtime"),
        };

        paths.prepare_runtime_dir().unwrap();

        let metadata = fs::symlink_metadata(&paths.runtime_dir).unwrap();

        assert!(metadata.is_dir());
        assert!(!metadata.file_type().is_symlink());
        assert_eq!(metadata.permissions().mode() & 0o777, 0o700);

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_runtime_directory_symlinks() {
        use std::{fs, os::unix::fs::symlink};

        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let root = std::env::temp_dir().join(format!(
            "ariawatch-symlink-test-{}-{unique_id}",
            std::process::id()
        ));

        fs::create_dir_all(&root).unwrap();

        let real_dir = root.join("real");
        let runtime_link = root.join("runtime");

        fs::create_dir(&real_dir).unwrap();
        symlink(&real_dir, &runtime_link).unwrap();

        let paths = AppPaths {
            data_dir: root.join("data"),
            config_dir: root.join("config"),
            state_dir: root.join("state"),
            runtime_dir: runtime_link,
        };

        let error = paths.prepare_runtime_dir().unwrap_err();

        assert!(error.contains("symbolic link"));

        // The directory targeted by the symbolic link remains untouched.
        assert!(real_dir.is_dir());

        fs::remove_dir_all(root).unwrap();
    }
}
