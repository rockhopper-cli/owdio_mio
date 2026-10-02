// src/ui/config.rs
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::Sender;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub wav_default_path: PathBuf,
    pub max_chars_per_line: usize,
    pub subtitle_gap_ms: u32,

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
        if let Ok(contents) = fs::read_to_string(&path) {
            if let Ok(cfg) = toml::from_str::<AppConfig>(&contents) {
                return cfg;
            }
        }
        let mut legacy_path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        legacy_path.push("writehear");
        legacy_path.push("config.toml");
        if let Ok(contents) = fs::read_to_string(&legacy_path) {
            if let Ok(cfg) = toml::from_str::<AppConfig>(&contents) {
                return cfg;
            }
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

pub fn start_container(container_name: &str, log_tx: Option<Sender<String>>) {
    let name = container_name.to_string();
    std::thread::spawn(move || {
        let msg = format!("[Owdio Mio] Starting Podman container: {}", name);
        println!("{}", msg);
        if let Some(ref tx) = log_tx {
            let _ = tx.send(msg);
        }
        match Command::new("podman").args(["start", &name]).output() {
            Ok(output) if output.status.success() => {
                let msg = format!("[Owdio Mio] Podman container '{}' started.", name);
                println!("{}", msg);
                if let Some(ref tx) = log_tx {
                    let _ = tx.send(msg);
                }
            }
            Ok(output) => {
                let err_msg = format!(
                    "[Owdio Mio] Error starting container '{}': {}",
                    name,
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                eprintln!("{}", err_msg);
                if let Some(ref tx) = log_tx {
                    let _ = tx.send(err_msg);
                }
            }
            Err(e) => {
                let err_msg = format!("[Owdio Mio] Failed to invoke podman: {}", e);
                eprintln!("{}", err_msg);
                if let Some(ref tx) = log_tx {
                    let _ = tx.send(err_msg);
                }
            }
        }
    });
}

pub fn stop_container(container_name: &str, log_tx: Option<Sender<String>>) {
    let msg = format!("[Owdio Mio] Stopping Podman container: {}", container_name);
    println!("{}", msg);
    if let Some(ref tx) = log_tx {
        let _ = tx.send(msg);
    }
    let _ = Command::new("podman")
        .args(["stop", "-t", "3", container_name])
        .status();
}

// src/ui/config.rs

/// Helper to query Podman container state via CLI without repetitive boilerplate
fn get_container_status(container_name: &str) -> Option<String> {
    Command::new("podman")
        .args(["inspect", "--format", "{{.State.Status}}", container_name])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

enum ServiceStatus {
    Online,
    LoadingModel,
    Offline { container_status: Option<String> },
}

/// Probes an AI HTTP endpoint and falls back to Podman container inspection on failure
fn check_service_endpoint(
    client: &reqwest::blocking::Client,
    port: u16,
    container_name: &str,
    podman_available: bool,
) -> ServiceStatus {
    let url = format!("http://127.0.0.1:{}/health", port);
    match client.get(&url).send() {
        Ok(resp) if resp.status().is_success() => ServiceStatus::Online,
        Ok(resp) if resp.status().as_u16() == 503 => ServiceStatus::LoadingModel,
        _ => {
            let container_status = if podman_available {
                get_container_status(container_name)
            } else {
                None
            };
            ServiceStatus::Offline { container_status }
        }
    }
}

pub fn check_services(config: &AppConfig, log_tx: Sender<String>) {
    let cfg = config.clone();
    std::thread::spawn(move || {
        let _ = log_tx.send("[Health Check] Checking system dependencies and container status...".to_string());

        // 1. Check FFmpeg
        match Command::new("ffmpeg").arg("-version").output() {
            Ok(output) if output.status.success() => {
                let _ = log_tx.send("[Health Check] FFmpeg: Installed and available on PATH.".to_string());
            }
            _ => {
                let _ = log_tx.send("[Health Check] [Warning] FFmpeg: NOT FOUND on PATH! Audio segmentation will fail.".to_string());
            }
        }

        // 2. Check Podman CLI
        let podman_available = Command::new("podman")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if podman_available {
            let _ = log_tx.send("[Health Check] Podman: Available on PATH.".to_string());
        } else {
            let _ = log_tx.send("[Health Check] [Warning] Podman CLI not detected on PATH.".to_string());
        }

        let http_client = crate::podman::build_http_client(2);

        // 3. Check Whisper STT Endpoint
        let mut whisper_ready = false;
        match check_service_endpoint(&http_client, cfg.whisper_port, &cfg.whisper_container_name, podman_available) {
            ServiceStatus::Online => {
                let _ = log_tx.send(format!(
                    "[Health Check] [Success] Whisper STT service (port {}): ONLINE and ready.",
                    cfg.whisper_port
                ));
                whisper_ready = true;
            }
            ServiceStatus::LoadingModel => {
                let _ = log_tx.send(format!(
                    "[Health Check] Whisper STT service (port {}): LOADING MODEL (HTTP 503). Initializing in VRAM.",
                    cfg.whisper_port
                ));
            }
            ServiceStatus::Offline { container_status: Some(status) } => {
                let _ = log_tx.send(format!(
                    "[Health Check] [Warning] Whisper STT endpoint OFFLINE on port {}. Container '{}' status: '{}'.",
                    cfg.whisper_port, cfg.whisper_container_name, status
                ));
            }
            ServiceStatus::Offline { container_status: None } => {
                let _ = log_tx.send(format!(
                    "[Health Check] [Error] Whisper STT endpoint OFFLINE on port {}. Verify whisper-server is running.",
                    cfg.whisper_port
                ));
            }
        }

        // 4. Check Qwen LLM Endpoint
        match check_service_endpoint(&http_client, cfg.qwen_port, &cfg.qwen_container_name, podman_available) {
            ServiceStatus::Online => {
                let _ = log_tx.send(format!(
                    "[Health Check] [Success] Qwen LLM service (port {}): ONLINE and ready.",
                    cfg.qwen_port
                ));
            }
            ServiceStatus::LoadingModel => {
                let _ = log_tx.send(format!(
                    "[Health Check] Qwen LLM service (port {}): LOADING MODEL (HTTP 503).",
                    cfg.qwen_port
                ));
            }
            ServiceStatus::Offline { container_status: Some(status) } => {
                let _ = log_tx.send(format!(
                    "[Health Check] [Warning] Qwen LLM endpoint OFFLINE on port {}. Container '{}' status: '{}'.",
                    cfg.qwen_port, cfg.qwen_container_name, status
                ));
            }
            ServiceStatus::Offline { container_status: None } => {
                let _ = log_tx.send(format!(
                    "[Health Check] [Info] Qwen LLM endpoint OFFLINE on port {}. (Optional for subtitles, required for summary).",
                    cfg.qwen_port
                ));
            }
        }

        // 5. Final summary
        if whisper_ready {
            let _ = log_tx.send("[Health Check] [Success] Core transcription system is ONLINE.".to_string());
        } else {
            let _ = log_tx.send("[Health Check] [Warning] Whisper service is not ready. Start the container before submitting jobs.".to_string());
        }
    });
}

pub fn show(config: &mut AppConfig, log_tx: &Sender<String>, ui: &mut egui::Ui) {
    let mut changed = false;

    egui::ScrollArea::vertical()
        .id_salt("config_scroll_area")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.heading("Settings & Configuration");
            ui.add_space(10.0);

            // 1. General Subtitle & Audio Settings
            ui.label(egui::RichText::new("General Audio & Subtitle Defaults").strong());
            ui.add_space(4.0);

            egui::Grid::new("general_config_grid")
                .num_columns(2)
                .spacing([20.0, 12.0])
                .show(ui, |ui| {
                    ui.label("Default Media Directory:");
                    ui.horizontal(|ui| {
                        ui.monospace(config.wav_default_path.to_string_lossy().to_string());
                        if ui.button("📁 Browse...").clicked() {
                            let mut dialog = rfd::FileDialog::new();
                            if config.wav_default_path.exists() {
                                dialog = dialog.set_directory(&config.wav_default_path);
                            }
                            if let Some(folder) = dialog.pick_folder() {
                                config.wav_default_path = folder;
                                changed = true;
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("Max characters per line:");
                    if ui.add(egui::Slider::new(&mut config.max_chars_per_line, 10..=80).text("chars")).changed() {
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Subtitle gap:");
                    if ui.add(egui::DragValue::new(&mut config.subtitle_gap_ms).speed(10.0).suffix(" ms")).changed() {
                        changed = true;
                    }
                    ui.end_row();
                });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // 2. Whisper Server Settings
            ui.heading("Whisper Model (Speech-to-Text)");
            ui.add_space(4.0);

            egui::Grid::new("whisper_config_grid")
                .num_columns(2)
                .spacing([20.0, 12.0])
                .show(ui, |ui| {
                    ui.label("Whisper Port:");
                    ui.vertical(|ui| {
                        if ui.add(egui::DragValue::new(&mut config.whisper_port).range(1024..=65535)).changed() {
                            changed = true;
                        }
                        ui.label(
                            egui::RichText::new(format!("Inference URL: http://127.0.0.1:{}/inference", config.whisper_port))
                                .weak()
                                .small(),
                        );
                    });
                    ui.end_row();
                });

            ui.add_space(6.0);
            if ui.checkbox(&mut config.manage_whisper_container, "Manage local Whisper container").changed() {
                changed = true;
            }
            ui.label(
                egui::RichText::new("Start container on startup and stop on exit")
                    .weak()
                    .small(),
            );

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Container name:");
                let response = ui.add_enabled(
                    config.manage_whisper_container,
                    egui::TextEdit::singleline(&mut config.whisper_container_name).hint_text("whisper-server"),
                );
                if response.changed() {
                    changed = true;
                }

                if config.manage_whisper_container {
                    if ui.button("▶ Start").clicked() {
                        start_container(&config.whisper_container_name, Some(log_tx.clone()));
                    }
                    if ui.button("⏹ Stop").clicked() {
                        stop_container(&config.whisper_container_name, Some(log_tx.clone()));
                    }
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // 3. Qwen 2.5 Coder Settings
            ui.heading("Qwen 2.5 Coder (Notes & Summarization)");
            ui.add_space(4.0);

            egui::Grid::new("qwen_config_grid")
                .num_columns(2)
                .spacing([20.0, 12.0])
                .show(ui, |ui| {
                    ui.label("Qwen LLM Port:");
                    ui.vertical(|ui| {
                        if ui.add(egui::DragValue::new(&mut config.qwen_port).range(1024..=65535)).changed() {
                            changed = true;
                        }
                        ui.label(
                            egui::RichText::new(format!("Endpoint: http://127.0.0.1:{}/v1/chat/completions", config.qwen_port))
                                .weak()
                                .small(),
                        );
                    });
                    ui.end_row();
                });

            ui.add_space(6.0);
            if ui.checkbox(&mut config.manage_qwen_container, "Manage local Qwen container").changed() {
                changed = true;
            }
            ui.label(
                egui::RichText::new("Start container on startup and stop on exit")
                    .weak()
                    .small(),
            );

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Container name:");
                let response = ui.add_enabled(
                    config.manage_qwen_container,
                    egui::TextEdit::singleline(&mut config.qwen_container_name).hint_text("qwen-coder"),
                );
                if response.changed() {
                    changed = true;
                }

                if config.manage_qwen_container {
                    if ui.button("▶ Start").clicked() {
                        start_container(&config.qwen_container_name, Some(log_tx.clone()));
                    }
                    if ui.button("⏹ Stop").clicked() {
                        stop_container(&config.qwen_container_name, Some(log_tx.clone()));
                    }
                }
            });

            ui.add_space(16.0);
            if ui.button("🔄 Check All Services Status").clicked() {
                check_services(config, log_tx.clone());
            }

            ui.add_space(20.0);
        });

    if changed {
        if let Err(e) = config.save() {
            eprintln!("Failed to save config: {e}");
        }
    }
}