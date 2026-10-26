# AriaWatch

AriaWatch is a terminal-based download manager and live dashboard for [aria2](https://aria2.github.io/), written in Rust.

It provides a keyboard-driven interface for adding downloads, monitoring progress, pausing and resuming transfers, removing downloads, and viewing download history. It also includes a managed aria2 controller that can continue running after you exit the dashboard.

## Features

- **Live dashboard** — view download names, statuses, progress, speed, remaining size, ETA, destination, source URI, and errors.
- **Add downloads** — add HTTP/HTTPS URLs, supported magnet URIs, and local `.torrent` files.
- **Download controls** — pause and resume individual downloads or all downloads.
- **Removal options** — remove a download while keeping its files, or remove it and delete eligible downloaded files in managed mode.
- **Download history** — persist download records in SQLite, including status and removal information.
- **Session persistence** — managed aria2 uses a session file to restore unfinished downloads after a restart.
- **Managed controller** — start and supervise an aria2 process independently of the terminal dashboard.
- **External mode** — connect to an existing aria2 JSON-RPC endpoint without taking ownership of its process.
- **Paginated snapshots** — retrieve waiting and stopped downloads in batches rather than limiting each query to a single page.
- **Live updates** — combine WebSocket notifications with periodic polling and snapshot reconciliation.
- **Terminal navigation** — use arrow keys or Vim-style navigation.
- **Diagnostic logging** — write application logs to the user's state directory.

## Requirements

- Linux for the current managed-controller implementation.
- Rust toolchain supporting the Rust 2024 edition, if building from source.
- `aria2c` installed and available on `PATH` for managed mode.
- A terminal that supports the terminal features used by Crossterm and Ratatui.

External mode requires an already-running aria2 instance with JSON-RPC enabled. It does not need to launch or manage that instance.

## Prebuilt binary — Linux x86_64

The v0.3.0 release provides a prebuilt Linux x86_64 executable, so you do not need to install Rust or compile the project yourself.

Download `ariawatch-v0.3.0-linux-x86_64` from the [GitHub Releases page](https://github.com/AnkitKumarDhal/ariawatch/releases).

Open a terminal in the directory where you downloaded the file, then run:

```bash
chmod +x ariawatch-v0.3.0-linux-x86_64
./ariawatch-v0.3.0-linux-x86_64
```

No installer or package manager is required.

**Compatibility:** The v0.3.0 binary is built and tested on Arch Linux x86_64. It may work on other Linux distributions, but compatibility is not guaranteed. System library and ABI differences can affect whether the executable runs.

**Managed mode:** `aria2c` must still be installed and available on `PATH`. AriaWatch manages the aria2 process; it does not bundle aria2 itself.

This is an early binary release intended to make testing easier and gather feedback from Linux users. If the executable fails to start on your distribution, please report your distribution, architecture, and the error message.

## Build from source

Clone the repository:

```bash
git clone https://github.com/AnkitKumarDhal/ariawatch.git
cd ariawatch
```

Build the application:

```bash
cargo build --release
```

The resulting executable is:

```text
target/release/ariawatch
```

You can also run it during development:

```bash
cargo run
```

## Running AriaWatch

### Managed mode

Managed mode is the default. AriaWatch starts and supervises its own `aria2c` process.

Make sure `aria2c` is installed:

```bash
aria2c --version
```

Then start AriaWatch:

```bash
./target/release/ariawatch
```

Managed mode configures aria2's JSON-RPC endpoint locally and generates its own RPC secret. Keep the managed controller's runtime files private; the RPC secret is an authentication credential.

The managed download directory is:

```text
~/Downloads
```

When you quit the dashboard, the exit options let you choose whether the managed controller should keep running or stop.

- **Leave Running** — close the dashboard while the controller and aria2 continue running.
- **Stop Controller** — request a normal shutdown of the managed controller and its aria2 process.
- **Cancel** — return to the dashboard.

The controller supervises its managed aria2 child. If the controller disappears unexpectedly, aria2 is configured to stop when its supervising controller process is no longer running.

### External mode

Use external mode to connect to an aria2 instance that you have already started. AriaWatch does not start or stop that external process.

For example, start aria2 with JSON-RPC enabled:

```bash
aria2c \
    --enable-rpc=true \
    --rpc-listen-all=false \
    --rpc-listen-port=6800 \
    --rpc-secret='replace-with-a-long-token'
```

Then launch AriaWatch in external mode:

```bash
ARIA2_RPC_SECRET='replace-with-a-long-token' \
    ./target/release/ariawatch --external
```

The default RPC URL is:

```text
http://127.0.0.1:6800/jsonrpc
```

To use a different endpoint:

```bash
ARIA2_RPC_URL='http://127.0.0.1:6800/jsonrpc' \
ARIA2_RPC_SECRET='replace-with-a-long-token' \
    ./target/release/ariawatch --external
```

You can also provide the endpoint as a command-line option:

```bash
./target/release/ariawatch \
    --external \
    --rpc-url 'http://127.0.0.1:6800/jsonrpc'
```

When both are configured, `--rpc-url` takes precedence over `ARIA2_RPC_URL`. The RPC secret is read from `ARIA2_RPC_SECRET`.

The secret is optional if the external aria2 server does not require one. Using a secret is recommended.

**Security:** keep the RPC listener restricted to trusted interfaces. Do not expose an unauthenticated aria2 RPC endpoint to the public internet.

External mode can manage downloads through the RPC API, but deleting downloaded files through AriaWatch is restricted to managed mode. AriaWatch also never requests controller shutdown for an external connection.

## Keyboard controls

The controls below apply while the dashboard is active and no dialog is open.

| Key | Action |
| --- | --- |
| `↑` / `↓` | Move the selection |
| `k` / `j` | Move the selection up or down |
| `a` | Add a URL, magnet URI, or local `.torrent` file |
| `p` | Pause the selected download |
| `r` | Resume the selected paused download |
| `P` | Pause all downloads |
| `R` | Resume all downloads |
| `x` | Remove the selected download while keeping its files |
| `X` | Request removal and file deletion; available only in managed mode |
| `h` | Open download history |
| `q` / `Q` / `Esc` | Open exit options in managed mode, or quit the dashboard in external mode |

### Adding a download

Press `a`, enter a URL or magnet URI, or enter the path to a local `.torrent` file, then press `Enter`.

For a local torrent file, provide its path as accepted by the application. The current interface does not expand shell shortcuts such as `~` automatically, so use an absolute path if needed.

Press `Esc` to cancel the add dialog.

### Removing downloads

Press `x` to request removal while keeping the downloaded files.

Press `X` to request removal and deletion of eligible files in managed mode. Confirm the operation in the confirmation dialog before it proceeds.

File deletion is restricted to regular files inside the managed `~/Downloads` directory. AriaWatch refuses symbolic links and paths that resolve outside that directory. File deletion is not available in external mode.

### Download history

Press `h` to open the history view.

- Use `↑` / `↓` or `k` / `j` to navigate.
- Press `r` to refresh history.
- Press `h` or `Esc` to close the history view.

History is stored separately from aria2's session file. It records information about downloads and their last known status; it is not itself a mechanism for resuming transfers.

## Persistence and runtime files

AriaWatch follows the XDG directory conventions where applicable, with home-directory fallbacks.

| Data | Location |
| --- | --- |
| Application data | `$XDG_DATA_HOME/ariawatch`, or `~/.local/share/ariawatch` |
| Managed aria2 session | `aria2.session` in the application data directory |
| Managed download history | `history.sqlite3` in the application data directory |
| External endpoint history | An endpoint-specific `history-external-*.sqlite3` file in the application data directory |
| Configuration directory | `$XDG_CONFIG_HOME/ariawatch`, or `~/.config/ariawatch` |
| Application logs | `$XDG_STATE_HOME/ariawatch/ariawatch.log`, or `~/.local/state/ariawatch/ariawatch.log` |
| Runtime files | `$XDG_RUNTIME_DIR/ariawatch`, or `~/.cache/ariawatch/runtime` |

The runtime directory contains files used by the managed controller, including its Unix socket, PID record, lock, and RPC secret. Treat these as private runtime data.

The log rotates when the active log reaches approximately 1 MiB. The previous log is retained as `ariawatch.log.1`; older backups are overwritten during subsequent rotations.

### Session recovery

The managed controller configures aria2 to load its session file on startup and periodically save unfinished or failed downloads.

Periodic session saving means that an abrupt termination can lose changes made since the last save. Normal shutdown and forced termination are not equivalent, and session recovery should not be treated as a guarantee that every transfer will be restored perfectly.

SQLite history and the aria2 session file serve different purposes:

- **SQLite history** stores records for the dashboard's history view.
- **The aria2 session file** helps aria2 restore unfinished transfers.

## Snapshot reconciliation and pagination

AriaWatch fetches active downloads and retrieves waiting and stopped downloads in pages of up to 1,000 entries.

A complete snapshot is used to reconcile current downloads with saved history. If a download previously recorded as active, queued, or paused is missing from a complete snapshot, its history status can be marked `Unavailable`.

When a snapshot contains entries that cannot be parsed, AriaWatch preserves valid entries but avoids treating that partial result as authoritative for history reconciliation.

The number of stopped results available still depends on what aria2 retains. Pagination cannot recover stopped results that aria2 has already discarded, for example due to its `--max-download-result` setting.

## Troubleshooting

### `aria2c` was not found

Install aria2 using your distribution's package manager, and confirm that `aria2c` is available on `PATH`.

### AriaWatch cannot connect to aria2

For external mode, verify the RPC URL, whether the server requires a secret, and whether the endpoint is reachable.

Check that the RPC listener is enabled on aria2 and that its network binding matches your intended setup.

### A download does not appear in history as expected

Check the application logs for snapshot or history errors. A malformed or incomplete snapshot is deliberately not treated as a complete inventory.

### The controller fails to start

Check the application log for controller startup errors. Confirm that `aria2c` is installed and that no conflicting process is already using the configured RPC port.

### The prebuilt binary does not start

Confirm that you downloaded the Linux x86_64 binary and that your distribution provides compatible system libraries. If it still fails, include the error output and your distribution details when reporting the issue.

## Reporting bugs and feedback

AriaWatch v0.3.0 is an early release, and real-world testing is appreciated.

Please report bugs, unexpected behaviour, and feature requests through [GitHub Issues](https://github.com/AnkitKumarDhal/ariawatch/issues). Include your Linux distribution, architecture, steps to reproduce the issue, and relevant logs when possible. Do not post RPC secrets or other credentials.

## References

- [aria2 website and documentation](https://aria2.github.io/)
- [aria2 manual](https://aria2.github.io/manual/en/html/aria2c.html)
- [aria2 JSON-RPC documentation](https://github.com/aria2/aria2/blob/master/doc/manual-src/en/aria2c.rst)

## License

AriaWatch is licensed under the MIT License. See [LICENSE](LICENSE) for details.
