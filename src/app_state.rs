use crate::domain::download::Download;

pub struct AppState {
    downloads: Vec<Download>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            downloads: Vec::new(),
        }
    }

    pub fn add_download(&mut self, download: Download) -> Result<(), String> {
        for existing in &self.downloads {
            if existing.gid == download.gid {
                return Err(format!("Download with GID {} already exists", download.gid));
            }
        }

        self.downloads.push(download);
        Ok(())
    }

    pub fn download_count(&self) -> usize {
        self.downloads.len()
    }

    pub fn downloads(&self) -> &[Download] {
        &self.downloads
    }
}
