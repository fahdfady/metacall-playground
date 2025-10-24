use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Script {
    pub path: PathBuf,
    pub name: String,
    pub language: String,
    pub runtime: String,
    pub functions: Vec<String>,
    pub loaded: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub function: String,
    pub args: Vec<String>,
    pub output: String,
    pub duration_ms: u64,
    pub success: bool,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStep {
    pub id: String,
    pub script: String,
    pub function: String,
    pub args: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum View {
    ScriptBrowser,
    FunctionTester,
    PipelineBuilder,
    ResultsExplorer,
    Export,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    EditingArgs,
    AddingStep,
    ExportName,
}

#[derive(Debug, Clone)]
pub struct FunctionInput {
    pub selected_function: usize,
    pub args: Vec<String>,
    pub current_arg: String,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    Success,
    Error,
    Warning,
}
