use crate::domain::status::Status;

#[derive(Debug, PartialEq)]
pub struct Download {
    pub gid: String,
    pub name: String,
    pub size: u64,
    pub downloaded: u64,
    pub speed: u64,
    pub status: Status,
}

impl Download {
    pub fn summary(&self) -> String {
        format!("{} - {} bytes/s", self.name, self.speed)
    }

    pub fn new(
        gid: String,
        name: String,
        size: u64,
        downloaded: u64,
        speed: u64,
        status: Status,
    ) -> Self {
        Self {
            gid,
            name,
            size,
            downloaded,
            speed,
            status,
        }
    }
}
