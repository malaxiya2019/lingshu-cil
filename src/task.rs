use crate::model::Task;
use serde::{Deserialize, Serialize};

/// A saved task record for session persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub project: String,
    pub description: String,
    pub created_at: String,
    pub status: String,
    pub tools_used: Vec<String>,
    pub files_changed: Vec<String>,
    pub last_action: String,
}

impl TaskRecord {
    pub fn from_task(task: &Task, project: &str) -> Self {
        Self {
            id: task.id.clone(),
            project: project.to_string(),
            description: task.description.clone(),
            created_at: task.created_at.clone(),
            status: task.status.to_string(),
            tools_used: Vec::new(),
            files_changed: Vec::new(),
            last_action: chrono::Utc::now().format("%H:%M:%S").to_string(),
        }
    }

    /// Save task record to ~/.lingshu/sessions/
    pub fn save(&self) -> Result<String, String> {
        let dir = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("lingshu")
            .join("sessions");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{}.json", self.id));
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, &json).map_err(|e| e.to_string())?;
        Ok(path.display().to_string())
    }

    /// Load a task record by id
    pub fn load(id: &str) -> Result<Self, String> {
        let dir = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("lingshu")
            .join("sessions");
        let path = dir.join(format!("{}.json", id));
        let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }

    /// List all saved task records
    pub fn list_all() -> Result<Vec<String>, String> {
        let dir = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("lingshu")
            .join("sessions");
        if !dir.exists() { return Ok(Vec::new()); }
        let mut ids = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".json") {
                    ids.push(name.trim_end_matches(".json").to_string());
                }
            }
        }
        Ok(ids)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Task, TaskStatus};

    #[test]
    fn test_task_record_from_task() {
        let task = Task {
            id: "test_123".to_string(),
            description: "Fix warnings".to_string(),
            status: TaskStatus::InProgress,
            created_at: "12:00:00".to_string(),
        };
        let record = TaskRecord::from_task(&task, "/project");
        assert_eq!(record.id, "test_123");
        assert_eq!(record.description, "Fix warnings");
        assert_eq!(record.project, "/project");
        assert_eq!(record.status, "in-progress");
        assert!(record.tools_used.is_empty());
        assert!(record.files_changed.is_empty());
    }

    #[test]
    fn test_task_record_done_status() {
        let task = Task {
            id: "test_456".to_string(),
            description: "Refactor module".to_string(),
            status: TaskStatus::Done,
            created_at: "13:00:00".to_string(),
        };
        let record = TaskRecord::from_task(&task, "/project");
        assert_eq!(record.status, "done");
    }

    #[test]
    fn test_task_record_pending_status() {
        let task = Task {
            id: "test_789".to_string(),
            description: "Pending task".to_string(),
            status: TaskStatus::Pending,
            created_at: "14:00:00".to_string(),
        };
        let record = TaskRecord::from_task(&task, "/project");
        assert_eq!(record.status, "pending");
    }

    #[test]
    fn test_task_record_save_load() {
        let task = Task {
            id: format!("test_save_{}", chrono::Utc::now().timestamp()),
            description: "Save test".to_string(),
            status: TaskStatus::Done,
            created_at: "15:00:00".to_string(),
        };
        let record = TaskRecord::from_task(&task, "/tmp/project");
        let path = record.save().expect("Should save");
        assert!(path.contains(&record.id));

        // Load it back
        let loaded = TaskRecord::load(&record.id).expect("Should load");
        assert_eq!(loaded.id, record.id);
        assert_eq!(loaded.description, "Save test");
        assert_eq!(loaded.project, "/tmp/project");
        assert_eq!(loaded.status, "done");

        // Clean up
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_task_record_load_nonexistent() {
        let result = TaskRecord::load("nonexistent_id_12345");
        assert!(result.is_err());
    }
}
