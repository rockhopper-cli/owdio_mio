// src/podman/mod.rs
pub mod client;
pub mod llm_client;

use reqwest::blocking::Client;
use std::time::Duration;

/// Standard timeout for local AI model inference (5 minutes)
pub const DEFAULT_INFERENCE_TIMEOUT_SECS: u64 = 300;

/// Central factory for constructing blocking HTTP clients with consistent fallback behavior
pub fn build_http_client(timeout_secs: u64) -> Client {
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .unwrap_or_else(|_| Client::new())
}