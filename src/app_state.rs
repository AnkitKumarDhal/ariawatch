use crate::domain::download::Download;

// In-memory collection of downloads, tracked by AriaWatch
pub struct AppState {
    downloads: Vec<Download>,
}

impl AppState {
    // Creates an empty application state
    pub fn new() -> Self {
        Self {
            downloads: Vec::new(),
        }
    }

    // Reject a download if its GID is already tracked
    fn ensure_unique_gid(&self, gid: &str) -> Result<(), String> {
        for existing in &self.downloads {
            if existing.gid == gid {
                return Err(format!("Download with GID {} already exists", gid));
            }
        }
        Ok(())
    }

    // Adds a download, rejecting duplicate GIDs
    pub fn add_download(&mut self, download: Download) -> Result<(), String> {
        self.ensure_unique_gid(&download.gid)?;
        self.downloads.push(download);
        Ok(())
    }

    // Returns the number of tracked downloads
    pub fn download_count(&self) -> usize {
        self.downloads.len()
    }

    // Provides read-only access to the tracked downloads
    pub fn downloads(&self) -> &[Download] {
        &self.downloads
    }
}
