use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Storage,
    Performance,
    Update,
    Security,
    System,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Good,
    Warning,
    Critical,
    Unknown,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Issue {
    pub id: String,
    pub category: Category,
    pub status: Status,
    pub title_key: String,
    pub why_key: String,
    pub evidence: String,
    /// Placeholder; Task 9 migrates this to a `FixId` enum.
    pub fix: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CheckResult {
    pub check_id: String,
    pub category: Category,
    pub issues: Vec<Issue>,
    pub error: Option<String>,
}
