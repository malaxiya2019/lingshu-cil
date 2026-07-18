use crate::model::{ModelConfig, PermissionMode, Task};
use chrono::Utc;
use std::fs;
use std::path::PathBuf;

/// A coding session (not chat) — for resuming work
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub id: String,
    pub project_dir: String,
    pub created_at: String,
    pub last_active: String,
    pub tasks: Vec<Task>,
    pub model: String,
    pub mode: PermissionMode,
}

impl Session {
    pub fn new(project_dir: &PathBuf, model: &ModelConfig, mode: PermissionMode) -> Self {
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        Self {
            id: format!("sess_{}", Utc::now().timestamp()),
            project_dir: project_dir.display().to_string(),
            created_at: now.clone(),
            last_active: now,
            tasks: Vec::new(),
            model: model.name.clone(),
            mode,
        }
    }

    /// Save session to disk (for resume)
    pub fn save(&self) -> Result<String, String> {
        let session_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lingshu")
            .join("sessions");
        fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;

        let path = session_dir.join(format!("{}.json", self.id));
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, &json).map_err(|e| e.to_string())?;
        Ok(path.display().to_string())
    }

    /// Load a session from disk
    pub fn load(id: &str) -> Result<Self, String> {
        let session_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lingshu")
            .join("sessions");
        let path = session_dir.join(format!("{}.json", id));
        let json = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }

    /// List all saved sessions
    pub fn list_all() -> Result<Vec<String>, String> {
        let session_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lingshu")
            .join("sessions");
        if !session_dir.exists() {
            return Ok(Vec::new());
        }
        let mut sessions = Vec::new();
        for entry in fs::read_dir(&session_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".json") {
                    sessions.push(name.trim_end_matches(".json").to_string());
                }
            }
        }
        Ok(sessions)
    }
}
