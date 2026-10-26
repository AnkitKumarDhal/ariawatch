// damn this file is the one dealing with sockets (this literally broke my brain)
use super::aria2_process::ManagedAria2;
use crate::{logging, paths::AppPaths};
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

// this limits commands from going larger than 32 bytes
const MAX_COMMAND_LENGTH: usize = 32;
const CLIENT_TIMEOUT: Duration = Duration::from_secs(2);
const CONTROLLER_POLL_TERMINAL: Duration = Duration::from_millis(100);

// this is everything that a server needs to manage its socket (i read that somewhere, i forgor)
struct BoundSocket {
    listener: UnixListener,
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl BoundSocket {
    // binds a socket,,, or more semantically, returns a BoundSocket
    fn bind(path: &Path) -> Result<Self, String> {
        // this boi checks if there is any other socket existing at that path
        remove_stale_socket(path)?;

        // actually binds the socket
        let listener = UnixListener::bind(path).map_err(|error| {
            format!(
                "Could not bind controller socket {}: {error}",
                path.display()
            )
        })?;

        // restricts access for all others
        if let Err(error) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
            drop(listener);
            let _ = fs::remove_file(path);

            return Err(format!(
                "COuld not secure controller socket {}: {error}",
                path.display()
            ));
        }

        let metadata = fs::symlink_metadata(path).map_err(|error| {
            format!(
                "COuld not inspect controller socket {}: {error}",
                path.display()
            )
        })?;

        if !metadata.file_type().is_socket() {
            drop(listener);
            return Err(format!(
                "Controller socket path is not a socket: {}",
                path.display()
            ));
        }

        Ok(Self {
            listener,
            path: path.to_path_buf(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

impl Drop for BoundSocket {
    fn drop(&mut self) {
        let is_our_socket = fs::symlink_metadata(&self.path).is_ok_and(|metadata| {
            metadata.file_type().is_socket()
                && metadata.dev() == self.device
                && metadata.ino() == self.inode
        });

        if is_our_socket {
            let _ = fs::remove_file(&self.path);
        }
    }
}

// does the work of checking and removing any pre-existing sockets at the intended location
fn remove_stale_socket(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_socket() {
                return Err(format!(
                    "Refusing to reduce a non-socket controller path: {}",
                    path.display()
                ));
            }

            let current_uid = unsafe { libc::geteuid() };

            if metadata.uid() != current_uid {
                return Err(format!(
                    "Controller socket is not owned by the current user: {}",
                    path.display()
                ));
            }

            fs::remove_file(path).map_err(|error| {
                format!(
                    "Could not remove stale controller socket {}: {error}",
                    path.display()
                )
            })?;
        }

        Err(error) if error.kind() == io::ErrorKind::NotFound => {}

        Err(error) => {
            return Err(format!(
                "Could not inspect controller socket {}: {error}",
                path.display()
            ));
        }
    }

    Ok(())
}

// this is the actual main function of this part,,, the actual server loop
// NOTE now that this function is updated, it still does the same thing, but it also checks up on
// the controller process periodically too
pub fn run(paths: &AppPaths, aria2: &mut ManagedAria2) -> Result<(), String> {
    let socket = BoundSocket::bind(&paths.controller_socket_file())?;

    socket
        .listener
        .set_nonblocking(true)
        .map_err(|error| format!("Could not configure controller socket: {error}"))?;

    logging::info("Controller IPC socket is ready");

    let mut stop_requested = false;

    loop {
        if !aria2.is_running()? {
            break;
        }
        match socket.listener.accept() {
            Ok((stream, _address)) => match handle_connection(stream) {
                Ok(true) => {
                    stop_requested = true;
                    break;
                }
                Ok(false) => {}
                Err(error) => {
                    logging::warn(&format!("IPC request failed: {error}"));
                }
            },
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                continue;
            }

            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(CONTROLLER_POLL_TERMINAL);
            }

            Err(error) => {
                return Err(format!("Controller socket accept failed: {error}"));
            }
        }
    }

    if stop_requested {
        logging::info("Controller IPC shutdown requested");
    } else {
        logging::warn("Managed aria2c exited; controller is shutting down");
    }

    Ok(())
}

// this *sends* the command (only one) to the running controller and reads its response
pub(super) fn send_command(paths: &AppPaths, command: &str) -> Result<String, String> {
    if command.is_empty()
        || command.len() > MAX_COMMAND_LENGTH
        || command.contains('\n')
        || command.contains('\r')
    {
        return Err(String::from("Invalid IPC command"));
    }

    let socket_path = paths.controller_socket_file();

    let mut stream = UnixStream::connect(&socket_path).map_err(|error| {
        format!(
            "Could not connect to controller socket {}: {error}",
            socket_path.display()
        )
    })?;

    stream
        .set_read_timeout(Some(CLIENT_TIMEOUT))
        .map_err(|error| format!("Could not set IPC read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(CLIENT_TIMEOUT))
        .map_err(|error| format!("Could not set IPC write timeout: {error}"))?;

    stream
        .write_all(command.as_bytes())
        .and_then(|()| stream.write_all(b"\n"))
        .map_err(|error| format!("Could not send IPC command: {error}"))?;

    read_command(&mut stream).map_err(|error| format!("Could not read IPC response: {error}"))
}

// run() runs the server loop, and this handle_connection() handles one connected client at a time
fn handle_connection(mut stream: UnixStream) -> Result<bool, String> {
    stream
        .set_read_timeout(Some(CLIENT_TIMEOUT))
        .map_err(|error| format!("Could not set IPC read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(CLIENT_TIMEOUT))
        .map_err(|error| format!("Could not set IPC write timeout: {error}"))?;

    let command = read_command(&mut stream)
        .map_err(|error| format!("Could not reead IPC command: {error}"))?;

    let (response, should_stop) = match command.as_str() {
        "PING" => ("PONG\n", false),
        "STOP" => ("STOPPING\n", true),
        _ => ("ERROR unknown command\n", false),
    };

    stream
        .write_all(response.as_bytes())
        .map_err(|error| format!("Could not write IPC response: {error}"))?;

    Ok(should_stop)
}

// this is the closest i got to system handling. it directly reads from the socket and interprets
// and turns them into a command string (i swear this multiline comment was made multiline by nvim
// automatically)
fn read_command(stream: &mut UnixStream) -> io::Result<String> {
    let mut command = Vec::with_capacity(MAX_COMMAND_LENGTH);

    loop {
        let mut byte = [0_u8; 1];
        match stream.read(&mut byte)? {
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "client disconnected before sending a complete command",
                ));
            }

            _ if byte[0] == b'\n' => break,

            _ => {
                if command.len() >= MAX_COMMAND_LENGTH {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "IPC command is too long",
                    ));
                }

                command.push(byte[0]);
            }
        }
    }

    if command.last() == Some(&b'\r') {
        command.pop();
    }

    String::from_utf8(command).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::{BoundSocket, MAX_COMMAND_LENGTH, handle_connection};
    use std::{
        fs,
        io::{Read, Write},
        os::unix::{
            fs::{FileTypeExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
        path::PathBuf,
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn test_path() -> (PathBuf, PathBuf) {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let root = std::env::temp_dir().join(format!(
            "ariawatch-ipc-test-{}-{unique_id}",
            std::process::id()
        ));

        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();

        let socket_path = root.join("controller.sock");
        (root, socket_path)
    }

    fn exchange(command: &'static str) -> (String, bool) {
        let (mut client, server) = UnixStream::pair().unwrap();

        let worker = thread::spawn(move || handle_connection(server).unwrap());

        client.write_all(command.as_bytes()).unwrap();

        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();

        (response, worker.join().unwrap())
    }

    #[test]
    fn ping_returns_pong_without_stopping() {
        assert_eq!(exchange("PING\n"), ("PONG\n".to_owned(), false));
    }

    #[test]
    fn stop_returns_stopping_and_requests_shutdown() {
        assert_eq!(exchange("STOP\n"), ("STOPPING\n".to_owned(), true));
    }

    #[test]
    fn unknown_commands_do_not_stop_the_controller() {
        assert_eq!(
            exchange("INVALID\n"),
            ("ERROR unknown command\n".to_owned(), false)
        );
    }

    #[test]
    fn rejects_oversized_commands() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let worker = thread::spawn(move || handle_connection(server));

        let oversized = format!("{}\n", "X".repeat(MAX_COMMAND_LENGTH + 1));
        client.write_all(oversized.as_bytes()).unwrap();

        assert!(worker.join().unwrap().is_err());
    }

    #[test]
    fn removes_stale_sockets_but_preserves_non_socket_files() {
        let (root, socket_path) = test_path();

        // Simulate a socket left behind by a crashed process.
        let old_listener = UnixListener::bind(&socket_path).unwrap();
        drop(old_listener);

        let new_socket = BoundSocket::bind(&socket_path).unwrap();
        assert!(
            fs::symlink_metadata(&socket_path)
                .unwrap()
                .file_type()
                .is_socket()
        );

        drop(new_socket);
        assert!(!socket_path.exists());

        // Never remove or overwrite an ordinary file at the socket path.
        fs::write(&socket_path, "keep me").unwrap();

        assert!(BoundSocket::bind(&socket_path).is_err());
        assert_eq!(fs::read_to_string(&socket_path).unwrap(), "keep me");

        fs::remove_dir_all(root).unwrap();
    }
}
