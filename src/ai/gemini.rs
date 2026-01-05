use crate::ai::{AiProvider, AiRequest, AiResponse};
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub role: String,
    pub parts: Vec<Part>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Part {
    Text {
        text: String,
        #[serde(rename = "thoughtSignature", skip_serializing_if = "Option::is_none")]
        thought_signature: Option<String>,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: FunctionCall,
        #[serde(rename = "thoughtSignature", skip_serializing_if = "Option::is_none")]
        thought_signature: Option<String>,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: FunctionResponse,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCall {
    pub name: String,
    pub args: Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FunctionResponse {
    pub name: String,
    pub response: Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON Schema
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentRequest {
    pub contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<Content>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentResponse {
    pub candidates: Option<Vec<Candidate>>,
    pub usage_metadata: Option<UsageMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub content: Content,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetadata {
    pub prompt_token_count: u32,
    pub candidates_token_count: Option<u32>,
    pub total_token_count: u32,
    #[serde(flatten)]
    pub extra: Option<std::collections::HashMap<String, Value>>,
}

pub struct GeminiClient {
    api_key: String,
    model: String,
    client: Client,
}

impl GeminiClient {
    pub fn new(model: String) -> Self {
        let api_key = std::env::var("LLM_API_KEY").unwrap_or_default();
        Self {
            api_key,
            model,
            client: Client::new(),
        }
    }

    pub async fn generate_content(
        &self,
        request: GenerateContentRequest,
    ) -> Result<GenerateContentResponse> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let res = self.client.post(&url).json(&request).send().await?;

        if !res.status().is_success() {
            let status = res.status();
            let error_text = res.text().await?;
            anyhow::bail!("Gemini API error ({}): {}", status, error_text);
        }

        let body_text = res.text().await?;
        match serde_json::from_str::<GenerateContentResponse>(&body_text) {
            Ok(response) => Ok(response),
            Err(e) => {
                anyhow::bail!("Failed to decode response: {}. Body: {}", e, body_text);
            }
        }
    }
}

#[async_trait]
impl AiProvider for GeminiClient {
    async fn completion(&self, request: AiRequest) -> Result<AiResponse> {
        let contents = vec![Content {
            role: "user".to_string(),
            parts: vec![Part::Text {
                text: request.prompt,
                thought_signature: None,
            }],
        }];

        let system_instruction = request.system_prompt.map(|s| Content {
            role: "user".to_string(),
            parts: vec![Part::Text {
                text: s,
                thought_signature: None,
            }],
        });

        let gen_req = GenerateContentRequest {
            contents,
            tools: None,
            system_instruction,
        };

        let resp = self.generate_content(gen_req).await?;

        let candidate = resp
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .ok_or_else(|| anyhow::anyhow!("No candidates returned from Gemini"))?;

        let mut content = String::new();
        for part in &candidate.content.parts {
            if let Part::Text { text, .. } = part {
                content.push_str(text);
            }
        }

        let usage = resp.usage_metadata.unwrap_or(UsageMetadata {
            prompt_token_count: 0,
            candidates_token_count: Some(0),
            total_token_count: 0,
            extra: None,
        });

        Ok(AiResponse {
            content,
            tokens_in: usage.prompt_token_count,
            tokens_out: usage.candidates_token_count.unwrap_or(0),
        })
    }
}
