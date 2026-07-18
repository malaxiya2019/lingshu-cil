use crate::model::Task;
use std::collections::HashMap;

/// Task context for tracking coding task execution
#[derive(Debug)]
pub struct TaskContext {
    pub tasks: Vec<Task>,
    pub current_task_id: Option<String>,
    pub history: HashMap<String, TaskHistory>,
}

#[derive(Debug, Clone)]
pub struct TaskHistory {
    pub task_id: String,
    pub description: String,
    pub iterations: usize,
    pub files_modified: Vec<String>,
    pub errors: Vec<String>,
}

impl TaskContext {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            current_task_id: None,
            history: HashMap::new(),
        }
    }

    pub fn add_task(&mut self, task: Task) {
        self.tasks.push(task);
    }

    pub fn current_task(&self) -> Option<&Task> {
        self.current_task_id
            .as_ref()
            .and_then(|id| self.tasks.iter().find(|t| t.id == *id))
    }

    pub fn record_error(&mut self, task_id: &str, error: String) {
        self.history
            .entry(task_id.to_string())
            .or_insert_with(|| TaskHistory {
                task_id: task_id.to_string(),
                description: String::new(),
                iterations: 0,
                files_modified: Vec::new(),
                errors: Vec::new(),
            })
            .errors
            .push(error);
    }
}
