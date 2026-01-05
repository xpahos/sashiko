use anyhow::Result;
use serde_json::Value;
use std::path::PathBuf;
use tokio::fs;

pub struct PromptRegistry {
    base_dir: PathBuf,
}

impl PromptRegistry {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn get_base_dir(&self) -> &PathBuf {
        &self.base_dir
    }

    pub async fn get_system_prompt(&self) -> Result<String> {
        let identity = "You're an expert Linux kernel developer and maintainer with deepk knowledge of Linux, Operating Sytems, modern hardware and Linux community standards and processes.";
        let json_protocol = r#"
## Output Format
You must respond with a valid JSON object. Do not include markdown code blocks (```json ... ```) around the output, just the raw JSON. The JSON must adhere to this schema:

{
  "analysis_trace": [
    "string" // Step-by-step reasoning
  ],
  "summary": "Brief summary of the patchset",
  "score": number, // 0-10, where 10 is perfect
  "verdict": "string", // "Reviewed-by", "Acked-by", "Changes Requested"
  "findings": [
    {
      "file": "string",
      "line": number,
      "severity": "string", // "High", "Medium", "Low", "Style"
      "message": "string", // Technical explanation
      "suggestion": "string" // Optional: suggested fix
    }
  ]
}
"#;
        Ok(format!("{}\n{}", identity, json_protocol))
    }

    pub async fn build_context_prompt(&self, patchset: &Value) -> Result<String> {
        // analyze patchset to find touched files and guess subsystems
        let mut instructions = Vec::new();
        instructions.push("Specific guidelines for this patchset:".to_string());

        // Always add technical patterns
        if let Ok(content) = fs::read_to_string(self.base_dir.join("technical-patterns.md")).await {
            instructions.push(format!("\n## Technical Patterns\n{}", content));
        }

        // Detect subsystems from touched files
        // We iterate over "patches" in the patchset JSON
        let patches = patchset["patches"].as_array();
        if let Some(_patches) = patches {
            // We don't have file lists in patchset summary usually, unless we parse diffs or have it stored.
            // The patchset details JSON has "patches" list.
            // We'd need to fetch file stats or just look at diffs if available.
            // For now, let's assume we can't easily get full file list without expensive calls.
            // But the Agent can call tools.
            // Here we are building the initial prompt.

            // If we can't detect, we just add general advice or ask the model to check.
            instructions.push(
                "Please analyze the touched files and apply relevant subsystem rules.".to_string(),
            );
        }

        // Add False Positive Guide
        if let Ok(content) = fs::read_to_string(self.base_dir.join("false-positive-guide.md")).await
        {
            instructions.push(format!("\n## False Positive Guide\n{}", content));
        }

        Ok(instructions.join("\n"))
    }
}
