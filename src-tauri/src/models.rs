use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusTarget {
    pub window_id: isize,
    pub process_id: u32,
    pub title: String,
    pub captured_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingConfig {
    pub device_id: Option<String>,
    pub provider: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub language: Option<String>,
    #[serde(default = "default_compute_device")]
    pub compute_device: String,
    #[serde(default)]
    pub hotwords: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingReview {
    pub session_id: String,
    pub text: String,
    pub duration: u64,
    pub hotwords: Vec<String>,
    pub engine: String,
    pub model: Option<String>,
}

fn default_compute_device() -> String {
    "auto".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub text: String,
    pub stable: bool,
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSession {
    pub id: String,
    pub state: String,
    pub target: FocusTarget,
    pub started_at: String,
    pub committed_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelInfo {
    pub id: String,
    pub engine: String,
    pub name: String,
    pub size: String,
    pub languages: String,
    pub speed: String,
    pub description: String,
    pub status: String,
    pub progress: u8,
    pub path: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelProgress {
    pub id: String,
    pub status: String,
    pub progress: u8,
    pub path: Option<String>,
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloaded_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_per_second: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    pub id: i64,
    pub name: String,
    pub trigger_word: String,
    pub prompt: String,
    pub enabled: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hotword {
    pub id: i64,
    pub word: String,
    pub category: String,
    pub behavior: String,
    pub replacement: Option<String>,
    pub priority: i64,
    pub apps: Vec<String>,
    pub hits: i64,
    pub last_used: String,
    pub enabled: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordItem {
    pub id: String,
    pub started_at: String,
    pub app: String,
    pub duration: u64,
    pub chars: usize,
    pub hotwords: Vec<String>,
    pub ai: bool,
    pub llm_provider: Option<String>,
    pub model: Option<String>,
    pub text: String,
    pub formatted_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmRequest {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub instructions: String,
    pub input: String,
    pub max_output_tokens: u32,
    #[serde(default)]
    pub thinking: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTest {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InjectionState {
    pub session_id: String,
    pub state: String,
    pub committed_text: String,
    pub message: Option<String>,
}
