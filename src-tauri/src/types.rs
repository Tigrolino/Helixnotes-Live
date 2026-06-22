use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMeta {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteEntry {
    pub path: String,
    pub relative_path: String,
    pub meta: NoteMeta,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashContents {
    pub notes: Vec<NoteEntry>,
    pub notebooks: Vec<TrashNotebookEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashNotebookEntry {
    pub name: String,
    pub path: String,
    pub note_count: usize,
    pub modified: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookEntry {
    pub name: String,
    pub path: String,
    pub relative_path: String,
    pub children: Vec<NotebookEntry>,
    pub note_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteContent {
    pub path: String,
    pub meta: NoteMeta,
    pub content: String,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub vaults: Vec<VaultConfig>,
    pub active_vault: Option<String>,
    pub theme: String,
    #[serde(default)]
    pub accent_color: Option<String>,
    #[serde(default)]
    pub font_size: Option<u32>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub line_height: Option<f64>,
    #[serde(default)]
    pub ui_scale: Option<f64>,
    #[serde(default)]
    pub content_width: Option<u32>,
    #[serde(default)]
    pub compact_notes: bool,
    #[serde(default = "default_true")]
    pub show_note_dates: bool,
    #[serde(default)]
    pub time_format: String,
    #[serde(default = "default_week_start")]
    pub week_start: String,
    #[serde(default = "default_daily_title_format")]
    pub daily_title_format: String,
    #[serde(default)]
    pub gpu_acceleration: bool,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub pdf_preview: bool,
    #[serde(default = "default_pdf_height")]
    pub pdf_height: u32,
    #[serde(default = "default_title_mode")]
    pub title_mode: String,
    #[serde(default)]
    pub hide_title_in_body: bool,
    #[serde(default)]
    pub show_line_numbers: bool,
    #[serde(default = "default_true")]
    pub show_link_arrows: bool,
    #[serde(default = "default_true")]
    pub show_all_notes: bool,
    #[serde(default = "default_true")]
    pub show_quick_access: bool,
    #[serde(default = "default_true")]
    pub show_tasks: bool,
    #[serde(default = "default_true")]
    pub show_daily_notes: bool,
    #[serde(default = "default_true")]
    pub show_trash: bool,
    #[serde(default)]
    pub backup_enabled: bool,
    #[serde(default = "default_backup_frequency")]
    pub backup_frequency: String,
    #[serde(default = "default_backup_max")]
    pub backup_max_count: u32,
    #[serde(default)]
    pub backup_location: Option<String>,
    #[serde(default)]
    pub last_backup_time: Option<String>,
    #[serde(default)]
    pub backup_include_attachments: bool,
    #[serde(default = "default_max_versions")]
    pub max_versions_per_note: u32,
    #[serde(default)]
    pub ai_provider: Option<String>,
    #[serde(default)]
    pub ai_api_key: Option<String>,
    #[serde(default)]
    pub openai_api_key: Option<String>,
    #[serde(default)]
    pub ollama_base_url: Option<String>,
    #[serde(default)]
    pub ollama_api_key: Option<String>,
    #[serde(default)]
    pub openai_compatible_base_url: Option<String>,
    #[serde(default)]
    pub openai_compatible_api_key: Option<String>,
    #[serde(default = "default_ai_model")]
    pub ai_model: String,
    #[serde(default)]
    pub ai_writing_style: Option<String>,
    #[serde(default)]
    pub default_view_mode: bool,
    #[serde(default)]
    pub show_tray_icon: bool,
    #[serde(default)]
    pub close_to_tray: bool,
    #[serde(default = "default_true")]
    pub enable_wiki_links: bool,
    #[serde(default)]
    pub restore_last_session: bool,
    // WebDAV sync (opt-in; all off by default). Endpoint is fully user-configured.
    #[serde(default)]
    pub sync_provider: Option<String>,
    #[serde(default)]
    pub webdav_url: Option<String>,
    #[serde(default)]
    pub webdav_username: Option<String>,
    #[serde(default)]
    pub webdav_password: Option<String>,
    #[serde(default)]
    pub sync_on_open: bool,
    #[serde(default)]
    pub sync_on_change: bool,
    #[serde(default)]
    pub sync_interval_minutes: u32,
    #[serde(default)]
    pub last_sync_time: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_pdf_height() -> u32 {
    600
}

fn default_backup_frequency() -> String {
    "24h".to_string()
}

fn default_backup_max() -> u32 {
    10
}

fn default_title_mode() -> String {
    "input".to_string()
}

fn default_week_start() -> String {
    "monday".to_string()
}

fn default_daily_title_format() -> String {
    "localized".to_string()
}

fn default_max_versions() -> u32 {
    20
}

fn default_ai_model() -> String {
    "claude-sonnet-4-6".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            vaults: Vec::new(),
            active_vault: None,
            theme: "system".to_string(),
            accent_color: None,
            font_size: None,
            font_family: None,
            line_height: None,
            ui_scale: None,
            content_width: None,
            compact_notes: false,
            show_note_dates: true,
            time_format: "relative".to_string(),
            week_start: "monday".to_string(),
            daily_title_format: "localized".to_string(),
            gpu_acceleration: true,
            autostart: false,
            pdf_preview: false,
            pdf_height: 600,
            title_mode: "input".to_string(),
            hide_title_in_body: false,
            show_line_numbers: false,
            show_link_arrows: true,
            show_all_notes: true,
            show_quick_access: true,
            show_tasks: true,
            show_daily_notes: true,
            show_trash: true,
            backup_enabled: false,
            backup_frequency: "24h".to_string(),
            backup_max_count: 10,
            backup_location: None,
            last_backup_time: None,
            backup_include_attachments: false,
            max_versions_per_note: 20,
            ai_provider: None,
            ai_api_key: None,
            openai_api_key: None,
            ollama_base_url: None,
            ollama_api_key: None,
            openai_compatible_base_url: None,
            openai_compatible_api_key: None,
            ai_model: "claude-sonnet-4-6".to_string(),
            ai_writing_style: None,
            default_view_mode: false,
            show_tray_icon: false,
            close_to_tray: false,
            enable_wiki_links: true,
            restore_last_session: false,
            sync_provider: None,
            webdav_url: None,
            webdav_username: None,
            webdav_password: None,
            sync_on_open: false,
            sync_on_change: false,
            sync_interval_minutes: 0,
            last_sync_time: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultState {
    pub last_open_note: Option<String>,
    pub sidebar_width: f64,
    pub notelist_width: f64,
    pub sidebar_collapsed: bool,
    #[serde(default)]
    pub notelist_collapsed: bool,
    #[serde(default)]
    pub collapsed_notebooks: Vec<String>,
    #[serde(default)]
    pub notebook_sort_mode: String,
    #[serde(default)]
    pub notebook_order: std::collections::HashMap<String, i32>,
    #[serde(default)]
    pub note_order: std::collections::HashMap<String, i32>,
    #[serde(default)]
    pub sort_mode: String,
    #[serde(default)]
    pub last_view_mode: String,
    #[serde(default)]
    pub last_notebook: Option<String>,
    #[serde(default)]
    pub last_tag: Option<String>,
    #[serde(default)]
    pub tasks_layout: String,
    #[serde(default = "default_true")]
    pub tasks_hide_completed: bool,
    #[serde(default)]
    pub tasks_only_flagged: bool,
    #[serde(default)]
    pub tasks_sort: String,
}

impl Default for VaultState {
    fn default() -> Self {
        Self {
            last_open_note: None,
            sidebar_width: 220.0,
            notelist_width: 280.0,
            sidebar_collapsed: false,
            notelist_collapsed: false,
            collapsed_notebooks: Vec::new(),
            notebook_sort_mode: String::new(),
            notebook_order: std::collections::HashMap::new(),
            note_order: std::collections::HashMap::new(),
            sort_mode: String::new(),
            last_view_mode: String::new(),
            last_notebook: None,
            last_tag: None,
            tasks_layout: String::new(),
            tasks_hide_completed: true,
            tasks_only_flagged: false,
            tasks_sort: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub path: String,
    pub title: String,
    pub snippet: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEvent {
    pub event_type: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub files_converted: u64,
    pub links_converted: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStats {
    pub total_notes: u64,
    pub total_attachments: u64,
    pub notes_size: u64,
    pub attachments_size: u64,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    pub filename: String,
    pub path: String,
    pub size: u64,
    pub created: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    pub timestamp: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiStreamEvent {
    pub event_type: String, // "text", "done", "error"
    pub text: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteTitleEntry {
    pub title: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub title: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: usize,
    pub target: usize,
    pub bidirectional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskItem {
    pub note_path: String,
    pub note_title: String,
    pub line: usize,
    pub raw_line: String,
    pub text: String,
    pub completed: bool,
    pub due: Option<String>,
    pub priority: Option<String>,
}
