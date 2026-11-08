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

- Linux (current managed-controller implementation).
- `aria2c` installed and available on `PATH` for managed mode.
- Rust toolchain supporting the Rust 2024 edition, if building from source.

## Installation

Prebuilt binaries, when available, can be found on the [GitHub Releases](https://github.com/AnkitKumarDhal/ariawatch/releases) page.

### Build from source

```bash
git clone https://github.com/AnkitKumarDhal/ariawatch.git
cd ariawatch
cargo build --release
```

Run the application:

```bash
./target/release/ariawatch
```

Managed mode is used by default. To connect to an existing aria2 instance, use external mode:

```bash
./target/release/ariawatch --external
```

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

## Persistence

AriaWatch stores application data using the XDG directory conventions, with home-directory fallbacks.

| Data | Default location |
| --- | --- |
| Application data | `~/.local/share/ariawatch` |
| Configuration | `~/.config/ariawatch` |
| Logs | `~/.local/state/ariawatch` |
| Runtime files | `$XDG_RUNTIME_DIR/ariawatch` or `~/.cache/ariawatch/runtime` |

Download history is stored in SQLite, while aria2's session file is used to restore unfinished downloads. These serve different purposes and are maintained separately.

### Configuration

AriaWatch automatically creates a default configuration file at:

`~/.config/ariawatch/config.toml`

You can customize the managed download directory:

```toml
[downloads]
directory = "/mnt/storage/Downloads"
```
Relative paths are resolved against the user's home directory.
Absolute paths are used as specified.
the default directory is `~/Downloads`

## Troubleshooting

**`aria2c` not found**

Install aria2 using your distribution's package manager and ensure `aria2c` is available on `PATH`.

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
