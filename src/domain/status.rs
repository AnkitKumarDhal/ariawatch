#[derive(Debug, PartialEq, Clone)]
pub enum Status {
    Active,
    Waiting,
    Paused,
    Complete,
    Error,
    Removed,
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "Downloading",
            Self::Waiting => "Queued",
            Self::Paused => "Paused",
            Self::Complete => "Completed",
            Self::Error => "Error",
            Self::Removed => "Removed",
        }
    }
}
