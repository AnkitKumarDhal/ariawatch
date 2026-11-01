# Changelog

All mentionable changes in AriaWatch are written here.

## [0.3.1] - 01-11-2026

### Fixed
- Fixed download deletion thingy across all the different download states
- Fix downloaded file deletion for paused and completed downloads
- Improve error reporting for failrd JSON-RPC requests.
- Improve terminal resotration when the TUI exits unexpectedly.

### Security
- Harden log file creation and rotation against symlink attacks.
- Apply restrictive permissions to application log files.
- Validate downloaded file paths before deletion.
- Refuse to delete symlinks and files outsdie the managed download directory.

### Internal
- Refactor aria2 integration into dedicated modules
- Separate monitoring, keyboard handling and TUI rendering into focused modules
- Simpllify the download domain model
- First initial pass at GitHub Actions CI for formatting, compilation, tests and clippy
