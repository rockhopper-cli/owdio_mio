// src/clients.rs
pub mod llm;
pub mod whisper;

use reqwest::blocking::Client;
use std::time::Duration;

pub const DEFAULT_INFERENCE_TIMEOUT_SECS: u64 = 300;

pub fn build_http_client(timeout_secs: u64) -> Client {
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .unwrap_or_else(|_| Client::new())
}
