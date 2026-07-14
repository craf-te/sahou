use serde::Serialize;

/// Positional structured diagnostic (spec §4). The byte-identical SoT shared across the 3 languages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Diag {
    pub code: String,
    pub path: String,
    pub message: String,
}

impl Diag {
    pub fn new(code: &str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            path: path.into(),
            message: message.into(),
        }
    }
}
