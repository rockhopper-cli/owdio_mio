// src/podman/llm_client.rs
use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use super::{build_http_client, DEFAULT_INFERENCE_TIMEOUT_SECS};

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: String,
}

pub struct LlmClient {
    endpoint: String,
    client: Client,
}

impl LlmClient {
    pub fn new(port: u16) -> Self {
        Self {
            endpoint: format!("http://127.0.0.1:{}/v1/chat/completions", port),
            client: build_http_client(DEFAULT_INFERENCE_TIMEOUT_SECS),
        }
    }

    pub fn complete(&self, system_prompt: &str, user_prompt: &str) -> Result<String> {
        let payload = ChatCompletionRequest {
            model: "qwen-coder".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_prompt.to_string(),
                },
            ],
            temperature: 0.3,
        };

        let res = self
            .client
            .post(&self.endpoint)
            .json(&payload)
            .send()
            .context("Failed to connect to Qwen LLM container. Ensure the container is running.")?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().unwrap_or_default();
            return Err(anyhow!("LLM server returned HTTP {}: {}", status, text));
        }

        let resp: ChatCompletionResponse = res
            .json()
            .context("Failed to parse JSON response from LLM server")?;

        resp.choices
            .first()
            .map(|c| c.message.content.trim().to_string())
            .ok_or_else(|| anyhow!("LLM returned empty completion choices"))
    }
}