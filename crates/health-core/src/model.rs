use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    /// Short technical detail for an expandable "details" section only.
    pub evidence: String,
    /// Language-neutral values (drive, percent, sizes) for i18n interpolation.
    #[serde(default)]
    pub params: BTreeMap<String, String>,
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
