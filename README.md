# AriaWatch

AriaWatch is a terminal dashboard for monitoring downloads managed by an existing [aria2](https://aria2.github.io/) instance.

It connects to aria2 through JSON-RPC and displays download progress in a live terminal interface. It is a **monitor, not a download manager**: it does not add, pause, resume, or remove downloads.

## Features

- Terminal dashboard showing download name, status, progress, speed, and remaining size.
- HTTP JSON-RPC for querying download information.
- WebSocket notifications for download lifecycle changes.
- Periodic progress updates and snapshot reconciliation.
- File-based diagnostic logging.
- Configurable RPC endpoint and optional authentication secret.

## Requirements

- Rust toolchain with support for the Rust 2024 edition.
- aria2 with its JSON-RPC server enabled.

## Configure aria2

Start aria2 with RPC enabled and a secret token:

```bash
aria2c --enable-rpc=true \
    --rpc-listen-all=false \
    --rpc-listen-port=6800 \
    --rpc-secret='replace-with-a-long-token'
```

The default AriaWatch endpoint is `http://127.0.0.1:6800/jsonrpc`.

Keep the RPC listener on the local loopback interface unless you have properly secured remote access. Do not expose the RPC port directly to the public internet.

## Build and run

Clone the repository and enter its directory:

```bash
git clone https://github.com/AnkitKumarDhal/ariawatch.git
cd ariawatch
```

Build the release binary:

```bash
cargo build --release
```

Run AriaWatch using the same secret configured for aria2:

```bash
env ARIA2_RPC_URL=http://127.0.0.1:6800/jsonrpc \
    ARIA2_RPC_SECRET='replace-with-a-long-token' \
    ./target/release/ariawatch
```

`ARIA2_RPC_URL` is optional when using the default endpoint. `ARIA2_RPC_SECRET` is optional when the aria2 server has no RPC secret configured, though using a secret is recommended.

Press `q`, `Q`, or `Esc` to quit the dashboard.

## Logging

AriaWatch writes diagnostic messages to:

- `$XDG_STATE_HOME/ariawatch/ariawatch.log` when `XDG_STATE_HOME` is set.
- `~/.local/state/ariawatch/ariawatch.log` otherwise.

The log contains timestamped application messages and diagnostic errors. Check it when the dashboard reports that an operation failed.
The active log is rotated when it reaches approximately 1 MiB. The previous log is retained as `ariawatch.log.1`; older backups are overwritten during subsequent rotations.

## Current limitations

- Waiting and stopped download lists are limited to the first 1,000 entries each; pagination is not implemented.
- AriaWatch monitors an existing aria2 instance and does not manage download actions.
- The application currently supports one configured aria2 endpoint.
- There is no way to scroll between downloads or look at more details for one specific download yet.

## References

- [aria2 documentation](https://aria2.github.io/manual/en/html/aria2c.html)
- [aria2 JSON-RPC documentation](https://github.com/aria2/aria2/blob/master/doc/manual-src/en/aria2c.rst)

## License

AriaWatch is licensed under the MIT License. See [LICENSE](LICENSE) for details.
