// src/podman/client.rs

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::{multipart, Client};
use serde::Deserialize;
use std::path::Path;
use super::{build_http_client, DEFAULT_INFERENCE_TIMEOUT_SECS};

#[derive(Debug, Clone)]
pub struct WhisperSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

// Structures matching whisper.cpp verbose_json output
#[derive(Debug, Deserialize)]
struct WhisperVerboseResponse {
    #[serde(default)]
    segments: Vec<WhisperApiSegment>,
}

#[derive(Debug, Deserialize)]
struct WhisperApiSegment {
    start: f64,
    end: f64,
    text: String,
}

pub struct WhisperClient {
    endpoint: String,
    client: Client,
}

impl WhisperClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            client: build_http_client(DEFAULT_INFERENCE_TIMEOUT_SECS),
        }
    }

    /// Sends a WAV chunk to whisper.cpp replicating:
    /// curl http://.../inference -F "file=@..." -F "response_format=verbose_json" -F "max_len=..." -F "split_on_word=true"
    pub fn send_chunk<P: AsRef<Path>>(
        &self,
        chunk_path: P,
        max_len: usize,
    ) -> Result<Vec<WhisperSegment>> {
        let chunk_path = chunk_path.as_ref();

        if !chunk_path.exists() {
            return Err(anyhow!("Chunk file does not exist: {}", chunk_path.display()));
        }

        // Equivalent to the multipart -F form fields in curl
        let form = multipart::Form::new()
            .file("file", chunk_path)
            .with_context(|| format!("Failed to attach audio file: {}", chunk_path.display()))?
            .text("response_format", "verbose_json")
            .text("max_len", max_len.to_string())
            .text("split_on_word", "true");

        let response = self
            .client
            .post(&self.endpoint) // e.g. "http://127.0.0.1:8081/inference"
            .multipart(form)
            .send()
            .context("Failed to connect to whisper.cpp server in Podman")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .unwrap_or_else(|_| String::from("<unable to read response>"));

            return Err(anyhow!(
                "whisper.cpp server returned HTTP {}:\n{}",
                status,
                body
            ));
        }

        let result: WhisperVerboseResponse = response
            .json()
            .context("Failed to parse verbose_json response from whisper.cpp")?;

        Ok(result
            .segments
            .into_iter()
            .map(|segment| WhisperSegment {
                start: segment.start,
                end: segment.end,
                text: segment.text.trim().to_string(),
            })
            .filter(|segment| !segment.text.is_empty())
            .collect())
    }
}