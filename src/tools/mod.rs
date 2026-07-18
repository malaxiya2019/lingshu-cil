use serde_json::Value;
use std::path::PathBuf;

pub mod file;
pub mod shell;
pub mod git;
pub mod search;
pub mod diagnose;

/// Unified Tool trait — every tool implements this
pub trait Tool {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;
    fn execute(&self, input: Value, project_dir: &PathBuf) -> ToolOutput;
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub output: String,
    pub is_error: bool,
}

/// Registry of all available tools
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool + Send + Sync>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let tools: Vec<Box<dyn Tool + Send + Sync>> = vec![
            Box::new(file::ReadFileTool),
            Box::new(file::WriteFileTool),
            Box::new(file::EditFileTool),
            Box::new(shell::RunShellTool),
            Box::new(search::SearchCodeTool),
            Box::new(file::ListDirTool),
            Box::new(git::GitStatusTool),
            Box::new(git::GitDiffTool),
            Box::new(diagnose::DiagnoseTool),
        ];
        Self { tools }
    }

    pub fn all(&self) -> &[Box<dyn Tool + Send + Sync>] {
        &self.tools
    }

    pub fn find(&self, name: &str) -> Option<&Box<dyn Tool + Send + Sync>> {
        self.tools.iter().find(|t| t.name() == name)
    }

    /// Execute a tool by name with the given JSON input
    pub fn execute(&self, name: &str, input: Value, project_dir: &PathBuf) -> ToolOutput {
        match self.find(name) {
            Some(tool) => tool.execute(input, project_dir),
            None => ToolOutput {
                output: format!("Unknown tool: {}", name),
                is_error: true,
            },
        }
    }

    /// Convert all tools to LLM tool definitions (for model.rs ToolDefinition compat)
    pub fn to_definitions(&self) -> Vec<crate::model::ToolDefinition> {
        self.tools
            .iter()
            .map(|t| crate::model::ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                input_schema: t.input_schema(),
            })
            .collect()
    }
}
