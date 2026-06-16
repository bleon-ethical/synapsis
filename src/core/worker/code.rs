use super::{Task, TaskError, TaskResult, TaskType, WorkerAgent};
use crate::core::lock_utils::*;
use crate::core::uuid::Uuid;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub struct CodeWorker {
    id: String,
    skills: Vec<String>,
    max_capacity: usize,
    last_heartbeat: Arc<Mutex<Instant>>,
}

impl Default for CodeWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeWorker {
    pub fn new() -> Self {
        Self {
            id: format!("code-{}", &Uuid::new_v4().to_hex_string()[..8]),
            skills: vec![
                "code_analysis".to_string(),
                "refactor".to_string(),
                "lint".to_string(),
                "format".to_string(),
            ],
            max_capacity: 2,
            last_heartbeat: Arc::new(Mutex::new(Instant::now())),
        }
    }

    fn analyze_code(&self, code: &str, language: Option<&str>) -> String {
        let mut analysis = serde_json::json!({
            "lines": code.lines().count(),
            "chars": code.len(),
        });

        let patterns = self.detect_patterns(code);
        analysis["patterns"] = serde_json::json!(patterns);

        if let Some(lang) = language {
            analysis["language"] = serde_json::json!(lang);
        }

        serde_json::to_string_pretty(&analysis).unwrap_or_default()
    }

    fn detect_patterns(&self, code: &str) -> Vec<String> {
        let mut patterns = Vec::new();

        if code.contains("TODO") || code.contains("FIXME") {
            patterns.push("has_todos".to_string());
        }
        if regex::Regex::new(r"\b\w+\s*\(.*\)\s*\{")
            .unwrap()
            .is_match(code)
        {
            patterns.push("has_functions".to_string());
        }
        if code.contains("impl ") {
            patterns.push("has_traits".to_string());
        }
        if code.contains("async fn") || code.contains("await") {
            patterns.push("async_code".to_string());
        }

        patterns
    }

    fn refactor_code(&self, code: &str, style: &str) -> Result<String, String> {
        match style {
            "fmt" => {
                let mut formatted = String::new();
                let mut indent = 0;
                for line in code.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        formatted.push('\n');
                        continue;
                    }
                    if trimmed.ends_with('{') {
                        formatted.push_str(&"    ".repeat(indent));
                        formatted.push_str(trimmed);
                        formatted.push('\n');
                        indent += 1;
                    } else if trimmed.starts_with('}') {
                        indent = indent.saturating_sub(1);
                        formatted.push_str(&"    ".repeat(indent));
                        formatted.push_str(trimmed);
                        formatted.push('\n');
                    } else {
                        formatted.push_str(&"    ".repeat(indent));
                        formatted.push_str(trimmed);
                        formatted.push('\n');
                    }
                }
                Ok(formatted)
            }
            _ => Err(format!("Unknown refactor style: {}", style)),
        }
    }
}

impl WorkerAgent for CodeWorker {
    fn id(&self) -> &str {
        &self.id
    }

    fn skills(&self) -> Vec<String> {
        self.skills.clone()
    }

    fn capacity(&self) -> usize {
        self.max_capacity
    }

    fn execute_task(&self, task: &Task) -> Result<TaskResult, TaskError> {
        *self.last_heartbeat.lock_safe() = Instant::now();
        let start = Instant::now();

        match task.task_type {
            TaskType::CodeAnalysis => {
                let code = task
                    .payload
                    .get("code")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| TaskError::new(0x0100, "Missing 'code' in payload", &task.id))?;

                let language = task.payload.get("language").and_then(|v| v.as_str());

                let analysis = self.analyze_code(code, language);
                Ok(TaskResult::success(
                    analysis,
                    start.elapsed().as_millis() as u64,
                ))
            }
            TaskType::CodeRefactor => {
                let code = task
                    .payload
                    .get("code")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| TaskError::new(0x0100, "Missing 'code' in payload", &task.id))?;

                let style = task
                    .payload
                    .get("style")
                    .and_then(|v| v.as_str())
                    .unwrap_or("fmt");

                match self.refactor_code(code, style) {
                    Ok(refactored) => Ok(TaskResult::success(
                        refactored,
                        start.elapsed().as_millis() as u64,
                    )),
                    Err(e) => Ok(TaskResult::failure(e, start.elapsed().as_millis() as u64)),
                }
            }
            _ => Err(TaskError::unsupported_operation(
                &format!("{:?}", task.task_type),
                &task.id,
            )),
        }
    }

    fn heartbeat(&self) -> bool {
        *self.last_heartbeat.lock_safe() = Instant::now();
        true
    }
}
