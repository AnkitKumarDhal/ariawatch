// this bad boi manmages the aria2 instance,,, now we're talking the whole download *management*
// setup, not just a monitor
use crate::{aria2, logging, paths::AppPaths};
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const RPC_ENDPOINT: &str = "http://127.0.0.1:6800/jsonrpc";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_INTERVAL: Duration = Duration::from_millis(50);

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
    pub(super) fn start(paths: &AppPaths) -> Result<Self, String> {
        paths.prepare_runtime_dir()?;
        prepare_private_dir(&paths.data_dir)?;

        let session_file = paths.aria2_session_file();
        prepare_session_file(&session_file)?;

        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| String::from("Could not determine the home directory"))?;
        let download_dir = home.join("Downloads");

        fs::create_dir_all(&download_dir).map_err(|error| {
            format!(
                "Could not create download directory {}: {error}",
                download_dir.display()
            )
        })?;

        let rpc_secret = generate_secret()?;
        let secret_file = RpcSecretFile::create(&paths.rpc_secret_file(), &rpc_secret)?;

        // YOU SEE THIS??? THIS GRINDED MY BRAIN SO BAD
        let child = Command::new("aria2c")
            .arg("--no-conf=true")
            .arg("--daemon=false")
            .arg("--enable-rpc=true")
            .arg("--rpc-listen-all=false")
            .arg("--rpc-listen-port=6800")
            .arg("--rpc-allow-origin-all=false")
            .arg(format!("--rpc-secret={rpc_secret}"))
            .arg(format!("--input-file={}", session_file.display()))
            .arg(format!("--save-session={}", session_file.display()))
            .arg("--save-session-interval=60")
            .arg("--continue=true")
            .arg("--quiet=true")
            .arg("--summary-interval=0")
            .arg(format!("--dir={}", download_dir.display()))
            .current_dir(&download_dir)
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

        if let Ok(request) =
            aria2::build_rpc_request("aria2.shutdown", Vec::new(), Some(&self.rpc_secret))
        {
            let endpoint = RPC_ENDPOINT.to_owned();

            thread::spawn(move || {
                if let Err(error) = aria2::send_rpc_request(&endpoint, &request) {
                    logging::warn(&format!(
                        "Graceful aria2 shutdown request returned an error: {error}"
                    ));
                }
            });
        }

        if self.wait_for_exit(SHUTDOWN_TIMEOUT) {
            self.stopped = true;
            return;
        }

        logging::warn("Managed aria2c did not stop in time; sending SIGTERM");

        unsafe {
            libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM);
        }

        if self.wait_for_exit(Duration::from_secs(2)) {
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
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
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

    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "Data path must be a real directory: {}",
            path.display()
        ));
    }

    let current_uid = unsafe { libc::geteuid() };

    if metadata.uid() != current_uid {
        return Err(format!(
            "Data directory is not owned by the current user: {}",
            path.display()
        ));
    }

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("Could not secure data directory: {error}"))?;

    Ok(())
}

// creates the aria2c session file
fn prepare_session_file(path: &Path) -> Result<(), String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| {
            format!(
                "Could not open aria2 session file {}: {error}",
                path.display()
            )
        })?;

    let metadata = file
        .metadata()
        .map_err(|error| format!("Could not inspect aria2 session file: {error}"))?;

    if !metadata.is_file() || metadata.uid() != unsafe { libc::geteuid() } {
        return Err(format!(
            "Aria2 session path must be a regular file owned by the current user: {}",
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

    Ok(())
}

// generates the rpc-secret
fn generate_secret() -> Result<String, String> {
    let mut randome_bytes = [0_u8; 32];

    File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut randome_bytes))
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
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(&temp_path)?;

        file.write_all(value.as_bytes())?;
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
    use super::{ManagedAria2, RpcSecretFile, generate_secret};
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

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
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let root = std::env::temp_dir().join(format!(
            "ariawatch-secret-test-{}-{unique_id}",
            std::process::id()
        ));

        fs::create_dir(&root).unwrap();

        let path = root.join("aria2-rpc.secret");
        let secret = RpcSecretFile::create(&path, "test-secret").unwrap();

        let metadata = fs::metadata(&path).unwrap();

        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::read_to_string(&path).unwrap(), "test-secret");

        drop(secret);

        assert!(!path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_when_managed_aria2_has_exited() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let root = std::env::temp_dir().join(format!(
            "ariawatch-dead-child-test-{}-{unique_id}",
            std::process::id()
        ));

        fs::create_dir(&root).unwrap();

        // Create a child that has already exited.
        let mut child = Command::new("sh").arg("-c").arg("exit 7").spawn().unwrap();

        let status = child.wait().unwrap();
        assert!(!status.success());

        // The secret path is deliberately absent in this test.
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
}
