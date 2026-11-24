# AriaWatch

**A modern, keyboard-driven TUI download manager for aria2, written in Rust.**

AriaWatch provides a terminal-based interface to manage and monitor your downloads without having to interact with aria2 directly.

## Features

- **Live dashboard** — monitor download progress, speed, ETA, status, and other details.
- **Download management** — add, pause, resume, and remove downloads.
- **Torrent support** — add magnet links and local `.torrent` files with file selection.
- **Batch imports** — import multiple direct-download URLs from plain-text link files.
- **Download history** — persistent download records backed by SQLite.
- **Session persistence** — restore unfinished downloads across restarts.
- **Managed controller** — manage an aria2 process independently of the dashboard.
- **External mode** — connect to an existing aria2 JSON-RPC instance.
- **Live updates** — WebSocket notifications combined with periodic polling.

## Requirements

### Supported platforms

- Linux x86_64
- Windows x86_64

### Managed mode

AriaWatch uses managed mode by default. It starts and manages a local aria2 process.

The `aria2c` executable must be available to AriaWatch:

- **Linux:** Install aria2 and ensure `aria2c` is available on `PATH`.
- **Windows portable:** `aria2c.exe` is bundled with the portable archive.
- **Windows standalone:** Install aria2 separately and ensure `aria2c.exe` is available on `PATH`.

AriaWatch checks for the aria2 executable beside its own executable first, then falls back to `PATH`.

### External mode

External mode connects to an existing aria2 instance through JSON-RPC. A local aria2 executable is not required in this mode.

### Building from source

- Rust 1.89 or newer.
- Cargo.

## Installation

### Prebuilt binaries

Prebuilt packages are published through [GitHub Releases](https://github.com/AnkitKumarDhal/ariawatch/releases).

The release workflow produces the following packages:

| Platform | Package | Contents |
| --- | --- | --- |
| Linux x86_64 | `.tar.gz` | AriaWatch binary, README and MIT license |
| Windows x86_64 | Portable `.zip` | AriaWatch, bundled aria2, documentation and third-party notices |
| Windows x86_64 | Standalone `.zip` | AriaWatch binary and documentation; requires aria2 installed separately |

#### Linux

1. Download the Linux x86_64 archive.
2. Extract the archive.
3. Ensure aria2 is installed and available on `PATH`.
4. Run the `ariawatch` executable.

#### Windows

**Portable package**

1. Download the Windows portable ZIP.
2. Extract the entire archive into a directory.
3. Keep `ariawatch.exe` and `aria2c.exe` together.
4. Run `ariawatch.exe`.

No separate aria2 installation is required.

**Standalone package**

1. Download and extract the standalone ZIP.
2. Install aria2 separately.
3. Ensure the directory containing `aria2c.exe` is available on `PATH`.
4. Run `ariawatch.exe`.

### Build from source

Clone the repository:

    git clone https://github.com/AnkitKumarDhal/ariawatch.git
    cd ariawatch

Build:

    cargo build --release --locked

Run on Linux:

    ./target/release/ariawatch

Run on Windows PowerShell:

    .\target\release\ariawatch.exe

Managed mode is used by default.

To connect to an existing aria2 instance, use external mode:

Linux:

    ./target/release/ariawatch --external

Windows PowerShell:

    .\target\release\ariawatch.exe --external

The default JSON-RPC endpoint is:

`http://127.0.0.1:6800/jsonrpc`

You can specify a different endpoint using `--rpc-url` or the `ARIA2_RPC_URL` environment variable.

The `ARIA2_RPC_SECRET` environment variable can be used to provide the RPC authentication secret.


## Keyboard shortcuts

| Key | Action |
| --- | --- |
| `↑` / `↓` or `j` / `k` | Navigate downloads |
| `a` | Add a download |
| `f` | View files in the selected download |
| `p` / `r` | Pause / resume selected download |
| `P` / `R` | Pause / resume all downloads |
| `x` | Remove download (keep files) |
| `X` | Remove download and delete files (managed mode) |
| `h` | Open download history |
| `q` / `Q` / `Esc` | Exit options or quit |

## Batch link-file imports

AriaWatch supports importing multiple direct-download URLs from `.txt`, `.links`, and `.list` files.

Each line should contain one URL. Blank lines and comments beginning with `#` are ignored.

Supported protocols: HTTP, HTTPS, FTP, FTPS, and SFTP.

Magnet links are handled separately through the torrent workflow.

### Torrent file selection

When adding a magnet link or local torrent file, AriaWatch
retrieves its metadata and opens a file-selection dialog.

| Key | Action |
| --- | --- |
| `↑` / `↓` or `j` / `k` | Navigate files |
| `Space` | Toggle file selection |
| `a` | Select all files |
| `n` | Deselect all files |
| `Enter` | Confirm and start download |
| `Esc` | Cancel |

## Persistence

AriaWatch stores application data using the XDG directory conventions, with home-directory fallbacks.


### Application directories

| Data | Linux | Windows |
| --- | --- | --- |
| Application data | `~/.local/share/ariawatch` | `%LOCALAPPDATA%\ariawatch` |
| Configuration | `~/.config/ariawatch` | `%APPDATA%\ariawatch` |
| State and logs | `~/.local/state/ariawatch` | `%LOCALAPPDATA%\ariawatch\state` |
| Runtime files | `$XDG_RUNTIME_DIR/ariawatch` or `~/.cache/ariawatch/runtime` | `%LOCALAPPDATA%\ariawatch\runtime` |

On Linux, AriaWatch respects the corresponding XDG environment variables when configured.

Download history is stored in SQLite, while aria2's session file is used to restore unfinished downloads. These serve different purposes and are maintained separately.



### Configuration

AriaWatch automatically creates a default configuration file.

Linux:

`~/.config/ariawatch/config.toml`

Windows:

`%APPDATA%\ariawatch\config.toml`

The default download directory is `Downloads` inside the user's home directory.

You can customize it:

    [downloads]
    directory = "~/Downloads"

For an absolute path, use the appropriate path for your operating system.

Linux:

    [downloads]
    directory = "/mnt/storage/Downloads"

Windows:

    [downloads]
    directory = "D:/Downloads"

Relative paths are resolved against the user's home directory. Absolute paths are used as specified.


## Troubleshooting

**`aria2c` not found**

AriaWatch could not locate the aria2 executable.

- On Linux, install aria2 and ensure `aria2c` is available on `PATH`.
- On Windows portable, ensure `aria2c.exe` is still beside `ariawatch.exe`.
- On Windows standalone, install aria2 and add its installation directory to `PATH`.

External mode does not require a local aria2 executable.

**Unable to connect to aria2**

For external mode, verify the RPC endpoint, port, and authentication secret. Ensure that JSON-RPC is enabled on the aria2 instance.

**Managed controller fails to start**

Check the application logs for startup errors. Confirm that aria2 is installed and that the configured RPC port is available.

**Prebuilt binary fails to launch**

Check that you downloaded the correct architecture and that your distribution provides compatible system libraries.

For additional help, report reproducible issues through [GitHub Issues](https://github.com/AnkitKumarDhal/ariawatch/issues).

## References

- [aria2 Website](https://aria2.github.io/)
- [aria2 Manual](https://aria2.github.io/manual/en/html/aria2c.html)
- [aria2 JSON-RPC Documentation](https://github.com/aria2/aria2/blob/master/doc/manual-src/en/aria2c.rst)

## Project

- **Repository:** [GitHub](https://github.com/AnkitKumarDhal/ariawatch)
- **Issues and feedback:** [GitHub Issues](https://github.com/AnkitKumarDhal/ariawatch/issues)
- **Documentation:** Detailed usage guides and configuration references will be available in the project Wiki.

## License

AriaWatch is licensed under the [MIT License](LICENSE).

