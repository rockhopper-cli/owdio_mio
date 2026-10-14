// src/config_struct.rs
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub wav_default_path: PathBuf,
    pub max_chars_per_line: usize,
    pub subtitle_gap_ms: u32,

    /// Chunk duration in seconds: 0 to 300 secs (0 = full file without splitting)
    pub whisper_chunk_duration_secs: u32,

    pub whisper_port: u16,
    pub manage_whisper_container: bool,
    pub whisper_container_name: String,

    pub qwen_port: u16,
    pub manage_qwen_container: bool,
    pub qwen_container_name: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            wav_default_path: dirs::audio_dir().unwrap_or_else(|| PathBuf::from(".")),
            max_chars_per_line: 42,
            subtitle_gap_ms: 100,
            whisper_chunk_duration_secs: 60, // 60 seconds (1 minute) default

            whisper_port: 8081,
            manage_whisper_container: false,
            whisper_container_name: "whisper-server".to_string(),

            qwen_port: 8080,
            manage_qwen_container: false,
            qwen_container_name: "qwen-coder".to_string(),
        }
    }
}

impl AppConfig {
    fn config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("owdiomio");
        path.push("config.toml");
        path
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(contents) = fs::read_to_string(&path)
            && let Ok(cfg) = toml::from_str::<AppConfig>(&contents) {
                return cfg;
            }

        // Backward compatibility fallback
        let mut legacy_path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        legacy_path.push("writehear");
        legacy_path.push("config.toml");
        if let Ok(contents) = fs::read_to_string(&legacy_path)
            && let Ok(cfg) = toml::from_str::<AppConfig>(&contents) {
                return cfg;
            }

        Self::default()
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)?;
        fs::write(path, toml_str)?;
        Ok(())
    }
}
