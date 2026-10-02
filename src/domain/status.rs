#[derive(Debug, PartialEq, Clone)]
pub enum Status {
    Active,
    Waiting,
    Paused,
    Complete,
    Error,
    Removed,
}
