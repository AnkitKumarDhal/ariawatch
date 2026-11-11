// this bad boi manmages the aria2 instance,,, now we're talking the whole download *management*
// setup, not just a monitor
use crate::{aria2, logging, paths::AppPaths};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

#[cfg(windows)]
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

const RPC_ENDPOINT: &str = "http://127.0.0.1:6800/jsonrpc";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_INTERVAL: Duration = Duration::from_millis(50);

#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

#[cfg(unix)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

pub(super) struct ManagedAria2 {
    child: Child,
    rpc_secret: String,
    _secret_file: RpcSecretFile,
    stopped: bool,
}

struct RpcSecretFile {
    path: PathBuf,
    value: String,
}

impl ManagedAria2 {
    // starts the managed aria2 service (finding the perfect set of arguments for the aria2c was
    // hell)
    pub(super) fn start(paths: &AppPaths, download_dir: &Path) -> Result<Self, String> {
        paths.prepare_runtime_dir()?;
        prepare_private_dir(&paths.data_dir)?;

        let session_file = paths.aria2_session_file();
        prepare_session_file(&session_file)?;

        fs::create_dir_all(download_dir).map_err(|error| {
            format!(
                "Could not create download directory {}: {error}",
                download_dir.display()
            )
        })?;

        let rpc_secret = generate_secret()?;
        let secret_file = RpcSecretFile::create(&paths.rpc_secret_file(), &rpc_secret)?;

        // YOU SEE THIS??? THIS GRINDED MY BRAIN SO BAD
        // update: just added another integrity feature with the stop-with-process arg
        let child = Command::new("aria2c")
            .arg("--no-conf=true")
            .arg("--daemon=false")
            .arg(format!("--stop-with-process={}", std::process::id()))
            .arg("--enable-rpc=true")
            .arg("--rpc-listen-all=false")
            .arg("--rpc-listen-port=6800")
            .arg("--rpc-allow-origin-all=false")
            .arg(format!("--rpc-secret={rpc_secret}"))
            .arg(format!("--input-file={}", session_file.display()))
            .arg(format!("--save-session={}", session_file.display()))
            .arg("--save-session-interval=60")
            .arg("--bt-save-metadata=true")
            .arg("--bt-load-saved-metadata=true")
            .arg("--continue=true")
            .arg("--quiet=true")
            .arg("--summary-interval=0")
            .arg(format!("--dir={}", download_dir.display()))
            .current_dir(download_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    String::from(
                        "aria2c was not found on PATH. Install aria2 before starting AriaWatch.",
                    )
                } else {
                    format!("Could not start managed aria2c: {error}")
                }
            })?;

        let mut managed = Self {
            child,
            rpc_secret,
            _secret_file: secret_file,
            stopped: false,
        };

        managed.wait_until_ready()?;
        logging::info("Managed aria2c RPC is ready");

        Ok(managed)
    }

    // makes the controller wait until the rpc is ready
    fn wait_until_ready(&mut self) -> Result<(), String> {
        let request = aria2::build_get_version_request(Some(&self.rpc_secret))?;
        let deadline = Instant::now() + STARTUP_TIMEOUT;

        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .map_err(|error| format!("Could not inspect aria2c startup: {error}"))?
            {
                self.stopped = true;

                return Err(format!(
                    "aria2c exited before becoming ready: {status}. Check the AriaWatch logs"
                ));
            }

            if let Ok(body) = aria2::send_rpc_request(RPC_ENDPOINT, &request)
                && let Ok(response) = aria2::parse_rpc_response(&body)
                && let Ok(result) = response.into_result("ariawatch-1")
                && result
                    .get("version")
                    .and_then(serde_json::Value::as_str)
                    .is_some()
            {
                return Ok(());
            }

            if Instant::now() >= deadline {
                return Err(String::from(
                    "Managed aria2c did not become ready within 10 seconds",
                ));
            }

            thread::sleep(RETRY_INTERVAL);
        }
    }

    // does the same thing as above but for the exit status/signal
    fn wait_for_exit(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;

        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    logging::info(&format!("Managed arai2c exited: {status}"));
                    return true;
                }
                Ok(None) => {}
                Err(error) => {
                    logging::warn(&format!("Could not inspect arai2c shutdown: {error}"));
                    return false;
                }
            }

            if Instant::now() >= deadline {
                return false;
            }

            thread::sleep(RETRY_INTERVAL);
        }
    }

    // checks if the managed aria2c is running or not
    pub(super) fn is_running(&mut self) -> Result<bool, String> {
        match self.child.try_wait() {
            Ok(None) => Ok(true),
            Ok(Some(status)) => {
                self.stopped = true;

                logging::error(&format!("Managed aria2c exited unexpectedly: {status}"));

                Ok(false)
            }
            Err(error) => Err(format!("Could not inspect managed aria2c process: {error}")),
        }
    }

    // finally using the semaphore, mutex and all that shi i learn int OS
    fn shutdown_process(&mut self) {
        if self.stopped {
            return;
        }

        if let Ok(Some(status)) = self.child.try_wait() {
            logging::info(&format!("Managed arai2c had already exited: {status}"));
            self.stopped = true;
            return;
        }

        #[cfg(unix)]
        unsafe {
            libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM);
        }

        #[cfg(windows)]
        {
            let shutdown_result =
                aria2::build_rpc_request("aria2.shutdown", Vec::new(), Some(&self.rpc_secret))
                    .and_then(|request| aria2::send_rpc_request(RPC_ENDPOINT, &request))
                    .and_then(|response| aria2::parse_rpc_response(&response))
                    .and_then(|response| response.into_result("ariawatch-1").map(|_| ()));

            if let Err(error) = shutdown_result {
                logging::warn(&format!(
                    "Could not request graceful aria2c shutdown: {error}"
                ));
            }
        }

        if self.wait_for_exit(SHUTDOWN_TIMEOUT) {
            self.stopped = true;
            return;
        }

        logging::warn("Managed aria2c did not respond to SIGTERM; killing it");

        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stopped = true;
    }
}

impl Drop for ManagedAria2 {
    fn drop(&mut self) {
        self.shutdown_process();
    }
}

impl RpcSecretFile {
    // secrets mmm,,, kinda sus
    fn create(path: &Path, value: &str) -> Result<Self, String> {
        write_secret_atomically(path, value)?;

        Ok(Self {
            path: path.to_path_buf(),
            value: value.to_owned(),
        })
    }
}

impl Drop for RpcSecretFile {
    fn drop(&mut self) {
        let is_our_file = fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| metadata.is_file() && !is_reparse_point(&metadata))
            && fs::read_to_string(&self.path).is_ok_and(|contents| contents == self.value);

        if is_our_file {
            let _ = fs::remove_file(&self.path);
        }
    }
}

// creates data directory
fn prepare_private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| {
        format!(
            "Could not create data directory {}: {error}",
            path.display()
        )
    })?;

    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "Could not inspect data directory {}: {error}",
            path.display()
        )
    })?;

    if is_reparse_point(&metadata) || !metadata.is_dir() {
        return Err(format!(
            "Data path must be a real directory: {}",
            path.display()
        ));
    }

    #[cfg(unix)]
    {
        let current_uid = unsafe { libc::geteuid() };

        if metadata.uid() != current_uid {
            return Err(format!(
                "Data directory is not owned by the current user: {}",
                path.display()
            ));
        }

        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("Could not secure data directory: {error}"))?;
    }

    Ok(())
}

// creates the aria2c session file
fn prepare_session_file(path: &Path) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);

    #[cfg(unix)]
    options
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);

    #[cfg(windows)]
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);

    let file = options.open(path).map_err(|error| {
        format!(
            "Could not open aria2 session file {}: {error}",
            path.display()
        )
    })?;

    let metadata = file
        .metadata()
        .map_err(|error| format!("Could not inspect aria2 session file: {error}"))?;

    if is_reparse_point(&metadata) || !metadata.is_file() {
        return Err(format!(
            "Aria2 session path must be a regular file: {}",
            path.display()
        ));
    }

    #[cfg(unix)]
    {
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(format!(
                "aria2 sesion file must not be owned by current user: {}",
                path.display()
            ));
        }

        if metadata.nlink() != 1 {
            return Err(String::from(
                "Aria2 session file must not have multiple hard links",
            ));
        }

        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("Could not secure aria2 session file: {error}"))?;
    }

    Ok(())
}

// generates the rpc-secret
fn generate_secret() -> Result<String, String> {
    let mut randome_bytes = [0_u8; 32];

    getrandom::fill(&mut randome_bytes)
        .map_err(|error| format!("Could not generate RPC secret: {error}"))?;

    Ok(randome_bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

// and this writes the secret file,,, atomically means that it does it in one pass, not overlaying
// to any other function
fn write_secret_atomically(path: &Path, value: &str) -> Result<(), String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let temp_path = path.with_file_name(format!(
        ".aria2-rpc-secret.{}.{timestamp}.tmp",
        std::process::id()
    ));

    let result = (|| -> io::Result<()> {
        let mut options = OpenOptions::new();

        options.write(true).create_new(true);

        #[cfg(unix)]
        options
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);

        #[cfg(windows)]
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);

        let mut file = options.open(&temp_path)?;

        file.write_all(value.as_bytes())?;

        #[cfg(unix)]
        file.set_permissions(fs::Permissions::from_mode(0o600))?;

        file.sync_all()?;
        drop(file);

        fs::rename(&temp_path, path)?;

        Ok(())
    })();

    if let Err(error) = result {
        let _ = fs::remove_file(&temp_path);
        return Err(format!("Could not write RPC secret file: {error}"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ManagedAria2, RpcSecretFile, generate_secret, prepare_private_dir, prepare_session_file,
    };
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[cfg(unix)]
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn test_root(prefix: &str) -> std::path::PathBuf {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        std::env::temp_dir().join(format!("{prefix}-{}-{unique_id}", std::process::id()))
    }

    #[test]
    fn generated_secret_is_256_bits_encoded_as_hex() {
        let secret = generate_secret().unwrap();

        assert_eq!(secret.len(), 64);
        assert!(
            secret
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
    }

    #[test]
    fn secret_file_is_private_and_removed_on_drop() {
        let root = test_root("ariawatch-secret-test");
        fs::create_dir(&root).unwrap();

        let path = root.join("aria2-rpc.secret");
        let secret = RpcSecretFile::create(&path, "test-secret").unwrap();

        #[cfg(unix)]
        {
            let metadata = fs::metadata(&path).unwrap();
            assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        }

        assert_eq!(fs::read_to_string(&path).unwrap(), "test-secret");

        drop(secret);

        assert!(!path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_when_managed_aria2_has_exited() {
        let root = test_root("ariawatch-dead-child-test");
        fs::create_dir(&root).unwrap();

        #[cfg(unix)]
        let mut child = Command::new("sh").arg("-c").arg("exit 7").spawn().unwrap();

        #[cfg(windows)]
        let mut child = Command::new("cmd")
            .args(["/C", "exit", "7"])
            .spawn()
            .unwrap();

        let status = child.wait().unwrap();
        assert!(!status.success());

        let mut managed = ManagedAria2 {
            child,
            rpc_secret: String::new(),
            _secret_file: RpcSecretFile {
                path: root.join("aria2-rpc.secret"),
                value: String::new(),
            },
            stopped: false,
        };

        assert!(!managed.is_running().unwrap());
        assert!(managed.stopped);

        drop(managed);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prepares_private_data_directory() {
        let root = test_root("ariawatch-private-dir-test");
        let data_dir = root.join("data");

        prepare_private_dir(&data_dir).unwrap();

        assert!(data_dir.is_dir());

        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&data_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn session_file_is_preserved_and_secured() {
        let root = test_root("ariawatch-session-test");
        fs::create_dir(&root).unwrap();

        let path = root.join("aria2.session");
        let contents = "https://example.invalid/archive.zip\n";

        fs::write(&path, contents).unwrap();

        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        prepare_session_file(&path).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), contents);

        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_directory_as_session_file() {
        let root = test_root("ariawatch-session-directory-test");
        fs::create_dir_all(&root).unwrap();

        let path = root.join("aria2.session");
        fs::create_dir(&path).unwrap();

        assert!(prepare_session_file(&path).is_err());

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_session_file_symlinks_without_modifying_target() {
        let root = test_root("ariawatch-session-symlink-test");
        fs::create_dir(&root).unwrap();

        let target = root.join("target");
        let session_path = root.join("aria2.session");

        fs::write(&target, "keep this content").unwrap();
        symlink(&target, &session_path).unwrap();

        assert!(prepare_session_file(&session_path).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "keep this content");

        fs::remove_dir_all(root).unwrap();
    }
}
