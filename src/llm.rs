use anyhow::{anyhow, Context, Result};
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub enum LlmProvider {
    Gemini,  // gemini-cli
    Cursor,  // cursor CLI
    Custom(String), // custom command
}

pub struct LlmClient {
    provider: LlmProvider,
}

impl LlmClient {
    /// Create a new LLM client, auto-detecting available CLI tools
    pub fn from_env() -> Result<Self> {
        // Check for explicit provider override
        if let Ok(cmd) = std::env::var("DONER_LLM_CMD") {
            return Ok(Self {
                provider: LlmProvider::Custom(cmd),
            });
        }

        // Auto-detect available CLI tools
        if is_command_available("gemini") {
            return Ok(Self {
                provider: LlmProvider::Gemini,
            });
        }

        if is_command_available("cursor") {
            return Ok(Self {
                provider: LlmProvider::Cursor,
            });
        }

        Err(anyhow!(
            "No LLM CLI tool found. Install one of:\n  \
             - gemini-cli (https://github.com/google-gemini/gemini-cli)\n  \
             - cursor CLI\n  \
             Or set DONER_LLM_CMD to a custom command"
        ))
    }

    /// Generate a rich summary from pre-formatted issue list
    pub async fn summarize(&self, formatted_issues: &str) -> Result<String> {
        let prompt = format!(
            "Summarize the following completed tasks:\n\n{}\n\n\
             Provide a rich summary that:\n\
             1. Groups related work into themes\n\
             2. Highlights key accomplishments\n\
             3. Notes any significant patterns",
            formatted_issues
        );

        match &self.provider {
            LlmProvider::Gemini => self.call_gemini_cli(&prompt).await,
            LlmProvider::Cursor => self.call_cursor_cli(&prompt).await,
            LlmProvider::Custom(cmd) => self.call_custom_cli(cmd, &prompt).await,
        }
    }

    async fn call_gemini_cli(&self, prompt: &str) -> Result<String> {
        let full_prompt = format!("{}\n\n{}", SYSTEM_PROMPT, prompt);

        let output = Command::new("gemini")
            .arg("-p")
            .arg(&full_prompt)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("Failed to execute gemini-cli")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("gemini-cli failed: {}", stderr));
        }

        let result = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(result.trim().to_string())
    }

    async fn call_cursor_cli(&self, prompt: &str) -> Result<String> {
        let full_prompt = format!("{}\n\n{}", SYSTEM_PROMPT, prompt);

        // Cursor CLI uses stdin for prompts
        let child = Command::new("cursor")
            .arg("--prompt")
            .arg(&full_prompt)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to execute cursor CLI")?;

        let output = child
            .wait_with_output()
            .await
            .context("Failed to read cursor CLI output")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("cursor CLI failed: {}", stderr));
        }

        let result = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(result.trim().to_string())
    }

    async fn call_custom_cli(&self, cmd: &str, prompt: &str) -> Result<String> {
        let full_prompt = format!("{}\n\n{}", SYSTEM_PROMPT, prompt);

        // Parse the command - first word is the executable, rest are base args
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return Err(anyhow!("DONER_LLM_CMD is empty"));
        }

        let (executable, base_args) = (parts[0], &parts[1..]);

        let output = Command::new(executable)
            .args(base_args)
            .arg(&full_prompt)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context(format!("Failed to execute custom command: {}", cmd))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Custom LLM command failed: {}", stderr));
        }

        let result = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(result.trim().to_string())
    }
}

/// Check if a command is available in PATH
fn is_command_available(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

const SYSTEM_PROMPT: &str = r#"You are a technical writer summarizing completed software development tasks. 
Your goal is to create clear, concise summaries that highlight:
- What was accomplished
- The impact or value of the work
- Any patterns or themes across multiple tasks

Write in a professional but accessible tone. Group related work together when it makes sense.
Use bullet points for clarity. Keep the summary focused and avoid unnecessary jargon. 
Include links to the issues in the summary if available. 
Use heading 4 for each theme and avoid using heading 1 to 3. Do not use bold formatting on headings."#;
