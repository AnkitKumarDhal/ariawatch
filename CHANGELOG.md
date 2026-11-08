# Changelog

All mentionable changes in AriaWatch are written here.

## [0.4.0] - 08-11-2026

### Added
- Add TOML-based user configuration at `~/.config/ariawatch/config.toml`
- Add support for custom managed download directories through user configuration
- Add interactive file selection for magnet and local `.torrent` downloads
- Add a torrent prep progress dialog while fetching metadata
- Add a file-list dialog for vieweing individual files in multi-file downloads
- Add torrent statistics (seeders, peers, upload speed, uploaded data)
- Add a dedicated seeding status for completed torrents that are still uploading
- Add batch download importing from plain-text link files (`.txt`, `.links`, `.list`)

### Improved
- Move torrent prep and batch link imports to background workers to keep the TUI responsive
- Presercve downloaded magnet metadata across restarts
- Support `~/` expansion in download paths
- Display more accurate torrent names and download destinations
- Improve torrent preparation and metadata cleanup

### Fixed
- Fix torrent removal across different download states
- Fix torrent file deletion by correctly resolving download paths
- Remove stale aria2 downlaod results after removal
- Remove empty parent directories after successful file deletion
- Fix torrent metadata entries remaining in the download list after preparation failure
- Fix parsing of torrent file progress information

---

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
