// aria2 download states
#[derive(Debug, PartialEq)]
pub enum Status {
    Active,
    Waiting,
    Paused,
    Complete,
    Error,
    Removed,
}

impl Status {
    // Returns actual status text to be displayed
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

    // Converts aria2 status strings into our enum
    pub fn from_aria2(value: &str) -> Result<Self, String> {
        match value {
            "active" => Ok(Status::Active),
            "waiting" => Ok(Status::Waiting),
            "paused" => Ok(Status::Paused),
            "complete" => Ok(Status::Complete),
            "error" => Ok(Status::Error),
            "removed" => Ok(Status::Removed),
            _ => Err(format!("Unknown aria2 status: {}", value)),
        }
    }
}
