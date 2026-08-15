use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct AnalyzeRequest {
    pub submission_id: String,
    #[serde(default)]
    pub source_key: String,
    #[serde(default)]
    pub source_base64: Option<String>,
    #[serde(default)]
    pub checks: Vec<CheckSpec>,
    #[serde(default)]
    pub max_evidence_per_check: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct CheckSpec {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    pub capability: String,
    #[serde(default = "one")]
    pub min_occurrences: usize,
    #[serde(default)]
    pub expect: Expect,
    #[serde(default)]
    pub track: Option<String>,
}

fn one() -> usize {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Expect {
    #[default]
    Present,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Passed,
    Failed,
    Inconclusive,
}

impl Status {
    pub fn as_bool(self) -> Option<bool> {
        match self {
            Status::Passed => Some(true),
            Status::Failed => Some(false),
            Status::Inconclusive => None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AnalyzeResponse {
    pub submission_id: String,
    pub source: SourceInfo,
    pub checks: Vec<CheckResult>,
    pub capabilities_detected: Vec<String>,
    pub duration_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct SourceInfo {
    pub files_scanned: usize,
    pub code_files: usize,
    pub python_files: usize,
    pub notebook_files: usize,
    pub markup_files: usize,
    pub manifest_files: usize,
    pub files_skipped: usize,
    pub bytes_scanned: usize,
    pub root_stripped: Option<String>,
    pub parse_failures: Vec<ParseFailure>,
    pub truncated: bool,
    pub coverage: Coverage,
}

#[derive(Debug, Serialize)]
pub struct Coverage {
    pub languages_present: Vec<String>,
    pub languages_analysed: Vec<String>,
    pub languages_unsupported: Vec<String>,
    pub manifests_read: Vec<String>,
    pub manifests_unsupported: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ParseFailure {
    pub file: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    pub capability: String,
    pub expect: Expect,
    pub status: Status,
    pub passed: Option<bool>,
    pub occurrences: usize,
    pub evidence: Vec<Evidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub matched: String,
    pub snippet: String,
}

#[derive(Debug, Serialize)]
pub struct CapabilityInfo {
    pub id: &'static str,
    pub description: &'static str,
    pub languages: &'static [&'static str],
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub stage: &'static str,
    pub error: String,
}
