use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Severity {
    Error,
    Warn,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "ERROR"),
            Severity::Warn => write!(f, "WARN"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub line: usize,
    pub col: usize,
    pub kind: Option<String>,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub rule: String,
    pub severity: Severity,
    pub file: String,
    pub function: String,
    pub line: usize,
    pub col: usize,
    pub message: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncovered_returns: Option<Vec<Location>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_event: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<HashMap<String, String>>,
}

impl Finding {
    pub fn error(rule: impl Into<String>, file: impl Into<String>, function: impl Into<String>, line: usize, col: usize, message: impl Into<String>) -> Self {
        Finding {
            rule: rule.into(),
            severity: Severity::Error,
            file: file.into(),
            function: function.into(),
            line,
            col,
            message: message.into(),
            uncovered_returns: None,
            suggested_event: None,
            extra: None,
        }
    }

    pub fn warn(rule: impl Into<String>, file: impl Into<String>, function: impl Into<String>, line: usize, col: usize, message: impl Into<String>) -> Self {
        Finding {
            rule: rule.into(),
            severity: Severity::Warn,
            file: file.into(),
            function: function.into(),
            line,
            col,
            message: message.into(),
            uncovered_returns: None,
            suggested_event: None,
            extra: None,
        }
    }
}
