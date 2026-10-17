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

    // Updates existing download or tracks a new one
    pub fn upsert_download(&mut self, download: Download) {
        if let Some(existing) = self
            .downloads
            .iter_mut()
            .find(|existing| existing.gid == download.gid)
        {
            *existing = download;
        } else {
            self.downloads.push(download);
        }
    }

    // Replace the tracked list with a fresh validated snapshot
    pub fn replace_downloads(&mut self, downloads: Vec<Download>) -> Result<(), String> {
        let mut replacement = Self::new();

        for download in downloads {
            replacement.add_download(download)?;
        }

        self.downloads = replacement.downloads;

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
