use crate::domain::status::Status;

pub struct Download {
    pub gid: String,
    pub name: String,
    pub size: u64,
    pub downloaded: u64,
    pub speed: u64,
    pub status: Status,
}
