use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct DevlogSession {
    pub schema_version: String,
    pub session_id: String,
    pub timestamp: String,
    pub machine_id: String,
    pub project_dir: String,
    pub git: Option<GitInfo>,
    pub conversation: Vec<ConversationEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GitInfo {
    pub remote: Option<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ConversationEntry {
    pub role: String,
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_description: Option<String>,
}
