use super::{DEFAULT_INFERENCE_TIMEOUT_SECS, build_http_client};
use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;

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

pub fn clean_llm_output(output: &str) -> String {
    let trimmed = output.trim();
    if let Some(start_idx) = trimmed.find("```") {
        let after_fence = &trimmed[start_idx + 3..];
        // Strip optional syntax tags (e.g. ```srt, ```text, ```markdown)
        let content_start = if let Some(newline_idx) = after_fence.find('\n') {
            &after_fence[newline_idx + 1..]
        } else {
            after_fence
        };

        if let Some(end_idx) = content_start.rfind("```") {
            return content_start[..end_idx].trim().to_string();
        }
        return content_start.trim().to_string();
    }
    trimmed.to_string()
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
            temperature: 0.1,
        };

        let res = self
            .client
            .post(&self.endpoint)
            .json(&payload)
            .send()
            .context("Failed to connect to Qwen LLM container. Ensure container is running.")?;

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

    pub fn spellcheck<F>(&self, text: &str, mut on_progress: F) -> Result<String>
    where
        F: FnMut(usize, usize, &str),
    {
        let system_prompt = "You are a professional editor and speech-to-text proofreader.\n\
            Review the provided transcript and correct:\n\
            1. Obvious spelling mistakes, typos, and phonetic transcription mishearings.\n\
            2. Grammar, punctuation, and capitalization.\n\
            3. Repeated words from stuttering or speech hesitation.\n\n\
            CRITICAL CONSTRAINTS:\n\
            - Strictly PRESERVE all timestamps (e.g. '[MM:SS]') exactly where they are.\n\
            - Do NOT rephrase or alter spoken style/meaning.\n\
            - Do NOT add explanations or introductions. Output ONLY the corrected text.";

        let lines: Vec<&str> = text.lines().collect();
        if lines.len() <= 60 {
            on_progress(1, 1, "Proofreading full transcript in 1 batch...");
            let start = Instant::now();
            let res = self
                .complete(system_prompt, text)
                .map(|s| clean_llm_output(&s))?;
            on_progress(
                1,
                1,
                &format!("Completed in {:.1}s", start.elapsed().as_secs_f32()),
            );
            return Ok(res);
        }

        const LINES_PER_BATCH: usize = 100;
        let chunks: Vec<&[&str]> = lines.chunks(LINES_PER_BATCH).collect();
        let total = chunks.len();
        let mut processed_blocks = Vec::with_capacity(total);

        for (i, chunk) in chunks.iter().enumerate() {
            let batch_num = i + 1;
            on_progress(
                batch_num,
                total,
                &format!(
                    "Proofreading batch {}/{} (lines {}-{})...",
                    batch_num,
                    total,
                    i * LINES_PER_BATCH + 1,
                    (i * LINES_PER_BATCH) + chunk.len()
                ),
            );

            let start = Instant::now();
            let chunk_str = chunk.join("\n");
            let corrected = self.complete(system_prompt, &chunk_str)?;
            processed_blocks.push(clean_llm_output(&corrected));

            on_progress(
                batch_num,
                total,
                &format!(
                    "Batch {}/{} finished in {:.1}s",
                    batch_num,
                    total,
                    start.elapsed().as_secs_f32()
                ),
            );
        }

        Ok(processed_blocks.join("\n"))
    }

    pub fn spellcheck_srt<F>(&self, srt_content: &str, mut on_progress: F) -> Result<String>
    where
        F: FnMut(usize, usize, &str),
    {
        let normalized = srt_content.replace("\r\n", "\n");
        let blocks: Vec<&str> = normalized
            .split("\n\n")
            .map(|b| b.trim())
            .filter(|b| !b.is_empty())
            .collect();

        if blocks.is_empty() {
            return Ok(srt_content.to_string());
        }

        const BLOCKS_PER_BATCH: usize = 60;
        let chunks: Vec<&[&str]> = blocks.chunks(BLOCKS_PER_BATCH).collect();
        let total = chunks.len();
        let mut corrected_blocks = Vec::with_capacity(total);

        let system_prompt = "You are an expert subtitle editor and proofreader.\n\
            Review the provided SRT subtitle blocks and correct:\n\
            1. Spelling mistakes, typos, and phonetic mishearings (names, technical terminology).\n\
            2. Capitalization, punctuation, and grammar.\n\
            3. Stuttering or speech hesitation.\n\n\
            CRITICAL RULES:\n\
            - Keep EVERY subtitle index and timestamp ('00:00:00,000 --> 00:00:00,000') EXACTLY as written.\n\
            - Retain the exact SRT structure: index, timestamp, text, followed by a blank line.\n\
            - Do NOT add markdown codeblocks, notes, or explanations. Output ONLY the raw SRT blocks.";

        for (i, chunk) in chunks.iter().enumerate() {
            let batch_num = i + 1;
            let start_sub = i * BLOCKS_PER_BATCH + 1;
            let end_sub = start_sub + chunk.len() - 1;

            on_progress(
                batch_num,
                total,
                &format!(
                    "Proofreading batch {}/{} (subtitles #{}-#{})...",
                    batch_num, total, start_sub, end_sub
                ),
            );

            let start = Instant::now();
            let chunk_text = chunk.join("\n\n");
            let raw_res = self.complete(system_prompt, &chunk_text)?;
            let cleaned = clean_llm_output(&raw_res);
            corrected_blocks.push(cleaned);

            on_progress(
                batch_num,
                total,
                &format!(
                    "Batch {}/{} finished in {:.1}s",
                    batch_num,
                    total,
                    start.elapsed().as_secs_f32()
                ),
            );
        }

        Ok(corrected_blocks.join("\n\n") + "\n\n")
    }
}
