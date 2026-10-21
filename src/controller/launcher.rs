use crate::{controller::ipc, paths::AppPaths};
use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const RETRY_INTERVAL: Duration = Duration::from_secs(50);

// starts the controller if required and then waits for its response
#[allow(dead_code)] // just for the time being
pub(super) fn ensure_running(paths: &AppPaths) -> Result<(), String> {
    paths.prepare_runtime_dir()?;

    if let Ok(response) = ipc::send_command(paths, "PING") {
        if response == "PONG" {
            return Ok(());
        }

        return Err(format!("Unexpected response from controller: {response}"));
    }

    let executable = std::env::current_exe()
        .map_err(|error| format!("Could not locate AriaWatch executable: {error}"))?;

    let mut child = Command::new(executable)
        .arg("--controller")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not launch controller: {error}"))?;

    let deadline = Instant::now() + STARTUP_TIMEOUT;

    loop {
        match ipc::send_command(paths, "PING") {
            Ok(response) if response == "PONG" => return Ok(()),
            Ok(response) => {
                return Err(format!("Unexpected response from controller: {response}"));
            }
            Err(_) => {}
        }

        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Could not inspect controller startup: {error}"))?
            && !status.success()
        {
            return Err(format!(
                "Controller bootstrap process exited unsuccessfully: {status}"
            ));
        }

        if Instant::now() >= deadline {
            return Err(format!(
                "Controller did not become responsive within {} seconds",
                STARTUP_TIMEOUT.as_secs()
            ));
        }

        thread::sleep(RETRY_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::ensure_running;
    use crate::paths::AppPaths;
    use std::{
        fs,
        io::{Read, Write},
        os::unix::{fs::PermissionsExt, net::UnixListener},
        path::PathBuf,
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn reuses_an_existing_responsive_controller() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let root = std::env::temp_dir().join(format!(
            "ariawatch-launcher-test-{}-{unique_id}",
            std::process::id()
        ));

        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();

        let paths = AppPaths {
            data_dir: root.join("data"),
            config_dir: root.join("config"),
            state_dir: root.join("state"),
            runtime_dir: root.join("runtime"),
        };

        paths.prepare_runtime_dir().unwrap();

        let listener = UnixListener::bind(paths.controller_socket_file()).unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();

            let mut request = [0_u8; 5];
            stream.read_exact(&mut request).unwrap();

            assert_eq!(&request, b"PING\n");

            stream.write_all(b"PONG\n").unwrap();
        });

        ensure_running(&paths).unwrap();
        server.join().unwrap();

        fs::remove_dir_all(root).unwrap();
    }
}
