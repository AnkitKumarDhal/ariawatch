use std::{env, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub runtime_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self, String> {
        Self::from_values(
            env::var_os("HOME").map(PathBuf::from),
            env::var_os("XDG_DATA_HOME").map(PathBuf::from),
            env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            env::var_os("XDG_STATE_HOME").map(PathBuf::from),
            env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        )
    }

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
}
