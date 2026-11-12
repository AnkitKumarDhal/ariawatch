// damn this file is the one dealing with sockets (this literally broke my brain)
use super::aria2_process::ManagedAria2;
use crate::{logging, paths::AppPaths};
use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::fs;

#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};

#[cfg(windows)]
use interprocess::os::windows::{
    local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor,
};
use interprocess::{
    ConnectWaitMode,
    local_socket::{
        ConnectOptions, GenericFilePath, Listener as LocalSocketListener, ListenerNonblockingMode,
        ListenerOptions, ToFsName as _, traits::Listener as _,
    },
};
#[cfg(windows)]
use widestring::U16CString;

// this limits commands from going larger than 32 bytes
const MAX_COMMAND_LENGTH: usize = 32;
const CLIENT_TIMEOUT: Duration = Duration::from_secs(2);
const CONTROLLER_POLL_TERMINAL: Duration = Duration::from_millis(100);

// platform-specific endpoints helper
#[cfg(unix)]
fn ipc_endpoint(path: &Path) -> PathBuf {
    path.to_path_buf()
}

#[cfg(windows)]
fn ipc_endpoint(path: &Path) -> PathBuf {
    let normalized_path = path.to_string_lossy().to_lowercase();
    let mut hash = 0xcbf29ce484222325_u64;

    for byte in normalized_path.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    PathBuf::from(format!(r"\\.\pipe\ariawatch-controller-{hash:016x}"))
}

// i wrote this one directly from AI, its very alien to me
#[cfg(windows)]
fn controller_security_descriptor() -> Result<SecurityDescriptor, String> {
    let sddl = U16CString::from_str("D:P(A;;GA;;;OW)")
        .map_err(|error| format!("Could not encode controller security descriptor: {error}"))?;

    SecurityDescriptor::deserialize(sddl.as_ucstr())
        .map_err(|error| format!("Could not create controller security descriptor: {error}"))
}

// this is everything that a server needs to manage its socket (i read that somewhere, i forgor)
struct BoundSocket {
    listener: LocalSocketListener,

    #[cfg(unix)]
    path: PathBuf,

    #[cfg(unix)]
    device: u64,

    #[cfg(unix)]
    inode: u64,
}

impl BoundSocket {
    // binds a socket,,, or more semantically, returns a BoundSocket
    fn bind(path: &Path) -> Result<Self, String> {
        // this boi checks if there is any other socket existing at that path
        #[cfg(unix)]
        remove_stale_socket(path)?;

        let endpoint = ipc_endpoint(path);

        let name = endpoint
            .as_path()
            .to_fs_name::<GenericFilePath>()
            .map_err(|error| {
                format!(
                    "Could not create controller IPC endpoint {}: {error}",
                    endpoint.display()
                )
            })?;

        let options = ListenerOptions::new().name(name).reclaim_name(false);

        #[cfg(windows)]
        let options = options.security_descriptor(controller_security_descriptor()?);

        // actually binds the socket
        let listener = options.create_sync().map_err(|error| {
            format!(
                "Could not bind controller socket {}: {error}",
                path.display()
            )
        })?;

        #[cfg(unix)]
        {
            // restricts access for all others
            if let Err(error) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
                drop(listener);
                let _ = fs::remove_file(path);

                return Err(format!(
                    "COuld not secure controller socket {}: {error}",
                    path.display()
                ));
            }

            let metadata = match fs::symlink_metadata(&endpoint) {
                Ok(metadata) => metadata,
                Err(error) => {
                    drop(listener);
                    let _ = fs::remove_file(&endpoint);

                    return Err(format!(
                        "Could not inspect controller socket {}: {error}",
                        endpoint.display()
                    ));
                }
            };

            if !metadata.file_type().is_socket() {
                drop(listener);
                return Err(format!(
                    "Controller socket path is not a socket: {}",
                    path.display()
                ));
            }

            Ok(Self {
                listener,
                path: endpoint,
                device: metadata.dev(),
                inode: metadata.ino(),
            })
        }

        #[cfg(windows)]
        {
            Ok(Self { listener })
        }
    }
}

impl Drop for BoundSocket {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
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
}

// does the work of checking and removing any pre-existing sockets at the intended location
#[cfg(unix)]
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

// the stream config helper
fn configure_ipc_stream<S>(stream: &S) -> io::Result<()>
where
    S: interprocess::local_socket::traits::Stream,
{
    #[cfg(unix)]
    {
        stream.set_recv_timeout(Some(CLIENT_TIMEOUT))?;
        stream.set_send_timeout(Some(CLIENT_TIMEOUT))?;
    }

    #[cfg(windows)]
    stream.set_nonblocking(true)?;

    Ok(())
}

// this is the actual main function of this part,,, the actual server loop
// NOTE now that this function is updated, it still does the same thing, but it also checks up on
// the controller process periodically too
pub fn run(paths: &AppPaths, aria2: &mut ManagedAria2) -> Result<(), String> {
    let socket = BoundSocket::bind(&paths.controller_socket_file())?;

    socket
        .listener
        .set_nonblocking(ListenerNonblockingMode::Accept)
        .map_err(|error| format!("Could not configure controller socket: {error}"))?;

    logging::info("Controller IPC socket is ready");

    let mut stop_requested = false;

    loop {
        if !aria2.is_running()? {
            break;
        }
        match socket.listener.accept() {
            Ok(stream) => {
                if let Err(error) = configure_ipc_stream(&stream) {
                    logging::warn(&format!("Could not configure IPC connection: {error}"));
                    continue;
                }

                match handle_connection(stream) {
                    Ok(true) => {
                        stop_requested = true;
                        break;
                    }

                    Ok(false) => {}

                    Err(error) => {
                        logging::warn(&format!("IPC request failed: {error}"));
                    }
                }
            }

            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                continue;
            }

            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(CONTROLLER_POLL_TERMINAL);
            }

            Err(error) => return Err(format!("Controller socket accept failed: {error}")),
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
    let endpoint = ipc_endpoint(&socket_path);

    let name = endpoint
        .as_path()
        .to_fs_name::<GenericFilePath>()
        .map_err(|error| {
            format!(
                "Could not create controller IPC endpoint {}: {error}",
                endpoint.display()
            )
        })?;

    let mut stream = ConnectOptions::new()
        .name(name)
        .wait_mode(ConnectWaitMode::Timeout(CLIENT_TIMEOUT))
        .connect_sync()
        .map_err(|error| {
            format!(
                "Could not connect to controller endpoint {}: {error}",
                endpoint.display()
            )
        })?;

    configure_ipc_stream(&stream)
        .map_err(|error| format!("Could not configure IPC connection: {error}"))?;

    write_all_with_timeout(&mut stream, command.as_bytes())
        .and_then(|()| write_all_with_timeout(&mut stream, b"\n"))
        .map_err(|error| format!("Could not send IPC command: {error}"))?;

    read_command(&mut stream).map_err(|error| format!("Could not read IPC response: {error}"))
}

fn write_all_with_timeout<W: Write>(stream: &mut W, mut bytes: &[u8]) -> io::Result<()> {
    let deadline = Instant::now() + CLIENT_TIMEOUT;

    while !bytes.is_empty() {
        match stream.write(bytes) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "IPC stream accepted no bytes",
                ));
            }
            Ok(written) => bytes = &bytes[written..],
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "IPC write timedout",
                    ));
                }

                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error),
        }
    }

    Ok(())
}

// run() runs the server loop, and this handle_connection() handles one connected client at a time
fn handle_connection<S>(mut stream: S) -> Result<bool, String>
where
    S: Read + Write,
{
    let command = read_command(&mut stream)
        .map_err(|error| format!("Could not read IPC command: {error}"))?;

    let (response, should_stop) = match command.as_str() {
        "PING" => ("PONG\n", false),
        "STOP" => ("STOPPING\n", true),
        _ => ("ERROR unknown command\n", false),
    };

    write_all_with_timeout(&mut stream, response.as_bytes())
        .map_err(|error| format!("Could not write IPC response: {error}"))?;

    Ok(should_stop)
}

// this is the closest i got to system handling. it directly reads from the socket and interprets
// and turns them into a command string (i swear this multiline comment was made multiline by nvim
// automatically)
fn read_command<R: Read>(stream: &mut R) -> io::Result<String> {
    let mut command = Vec::with_capacity(MAX_COMMAND_LENGTH);
    let deadline = Instant::now() + CLIENT_TIMEOUT;

    loop {
        let mut byte = [0_u8; 1];
        match stream.read(&mut byte) {
            Ok(0) => {
                #[cfg(windows)]
                {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "IPC command timed out",
                        ));
                    }
                }

                #[cfg(not(windows))]
                {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "client disconnected before sending a complete command",
                    ));
                }
            }

            Ok(_) if byte[0] == b'\n' => break,

            Ok(_) => {
                if command.len() >= MAX_COMMAND_LENGTH {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "IPC command is too long",
                    ));
                }

                command.push(byte[0]);
            }

            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "IPC command timed out",
                    ));
                }
                thread::sleep(Duration::from_millis(5));
            }

            Err(error) => return Err(error),
        }
    }

    if command.last() == Some(&b'\r') {
        command.pop();
    }

    String::from_utf8(command).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::{
        BoundSocket, CLIENT_TIMEOUT, MAX_COMMAND_LENGTH, configure_ipc_stream, handle_connection,
        ipc_endpoint, read_command, write_all_with_timeout,
    };
    use interprocess::{
        ConnectWaitMode,
        local_socket::{ConnectOptions, GenericFilePath, ToFsName as _, traits::Listener as _},
    };
    use std::{
        fs,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        path::{Path, PathBuf},
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[cfg(unix)]
    use std::os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::UnixListener,
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

        #[cfg(unix)]
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();

        let socket_path = root.join("controller.sock");

        (root, socket_path)
    }

    fn exchange(command: &str) -> (String, bool) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let mut client = TcpStream::connect(address).unwrap();
        let (server, _) = listener.accept().unwrap();

        let worker = thread::spawn(move || handle_connection(server).unwrap());

        client.write_all(command.as_bytes()).unwrap();

        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();

        (response, worker.join().unwrap())
    }

    fn connect_local(path: &Path) -> interprocess::local_socket::Stream {
        let endpoint = ipc_endpoint(path);

        let name = endpoint.as_path().to_fs_name::<GenericFilePath>().unwrap();

        ConnectOptions::new()
            .name(name)
            .wait_mode(ConnectWaitMode::Timeout(CLIENT_TIMEOUT))
            .connect_sync()
            .unwrap()
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
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let mut client = TcpStream::connect(address).unwrap();
        let (server, _) = listener.accept().unwrap();

        let worker = thread::spawn(move || handle_connection(server));

        let oversized = format!("{}\n", "X".repeat(MAX_COMMAND_LENGTH + 1));
        client.write_all(oversized.as_bytes()).unwrap();

        assert!(worker.join().unwrap().is_err());
    }

    #[test]
    fn local_socket_transport_works_on_current_platform() {
        let (root, socket_path) = test_path();

        let socket = BoundSocket::bind(&socket_path).unwrap();

        let worker = thread::spawn(move || {
            let stream = socket.listener.accept().unwrap();

            configure_ipc_stream(&stream).unwrap();

            handle_connection(stream).unwrap()
        });

        let mut client = connect_local(&socket_path);

        configure_ipc_stream(&client).unwrap();

        write_all_with_timeout(&mut client, b"PING\n").unwrap();

        let response = read_command(&mut client).unwrap();

        assert_eq!(response, "PONG");
        assert!(!worker.join().unwrap());

        drop(client);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn removes_stale_sockets_but_preserves_non_socket_files() {
        let (root, socket_path) = test_path();

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

        fs::write(&socket_path, "keep me").unwrap();

        assert!(BoundSocket::bind(&socket_path).is_err());
        assert_eq!(fs::read_to_string(&socket_path).unwrap(), "keep me");

        fs::remove_dir_all(root).unwrap();
    }
}
