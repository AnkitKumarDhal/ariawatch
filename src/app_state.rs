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

    fn ensure_unique_gid(&self, gid: &str) -> Result<(), String> {
        for existing in &self.downloads {
            if existing.gid == gid {
                return Err(format!("Download with GID {} already exists", gid));
            }
        }
        Ok(())
    }

    pub fn add_download(&mut self, download: Download) -> Result<(), String> {
        self.ensure_unique_gid(&download.gid)?;
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
