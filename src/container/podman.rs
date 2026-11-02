use crate::clients::build_http_client;
use crate::config::AppConfig;
use std::process::Command;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

pub fn start_container(container_name: &str, log_tx: Option<Sender<String>>) {
    let name = container_name.to_string();
    thread::spawn(move || {
        if let Some(ref tx) = log_tx {
            let _ = tx.send(format!("[Podman] Starting container: {}", name));
        }

        match Command::new("podman").args(["start", &name]).output() {
            Ok(output) if output.status.success() => {
                if let Some(ref tx) = log_tx {
                    let _ = tx.send(format!("[Podman] Container '{}' started.", name));
                }
            }
            Ok(output) => {
                let err_msg = format!(
                    "[Podman] Error starting container '{}': {}",
                    name,
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                if let Some(ref tx) = log_tx {
                    let _ = tx.send(err_msg);
                }
            }
            Err(e) => {
                if let Some(ref tx) = log_tx {
                    let _ = tx.send(format!("[Podman] Failed to invoke podman CLI: {}", e));
                }
            }
        }
    });
}

pub fn stop_container(container_name: &str, log_tx: Option<Sender<String>>) {
    let name = container_name.to_string();
    thread::spawn(move || {
        stop_container_sync(&name);
        if let Some(ref tx) = log_tx {
            let _ = tx.send(format!("[Podman] Container '{}' stopped.", name));
        }
    });
}

/// Synchronous stop with a fast timeout (3s) to safely run inside Drop before process exit.
pub fn stop_container_sync(container_name: &str) {
    let _ = Command::new("podman")
        .args(["stop", "-t", "3", container_name])
        .output();
}

pub fn get_container_status(container_name: &str) -> Option<String> {
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

pub fn check_services(config: &AppConfig, log_tx: Sender<String>, delay_ms: u64) {
    let cfg = config.clone();
    thread::spawn(move || {
        if delay_ms > 0 {
            thread::sleep(Duration::from_millis(delay_ms));
        }

        let _ = log_tx.send("[Health Check] Probing dependencies and services...".into());

        // 1. FFmpeg
        match Command::new("ffmpeg").arg("-version").output() {
            Ok(o) if o.status.success() => {
                let _ = log_tx.send("[Health Check] FFmpeg: Available on PATH.".into());
            }
            _ => {
                let _ = log_tx.send("[Health Check] [Warning] FFmpeg: NOT FOUND on PATH!".into());
            }
        }

        // 2. Podman CLI
        let podman_available = Command::new("podman")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if podman_available {
            let _ = log_tx.send("[Health Check] Podman: Available on PATH.".into());
        } else {
            let _ = log_tx.send("[Health Check] [Warning] Podman CLI not detected on PATH.".into());
        }

        let client = build_http_client(2);

        // 3. Whisper STT
        match check_service_endpoint(
            &client,
            cfg.whisper_port,
            &cfg.whisper_container_name,
            podman_available,
        ) {
            ServiceStatus::Online => {
                let _ = log_tx.send(format!(
                    "[Health Check] [Success] Whisper STT (port {}): ONLINE and ready.",
                    cfg.whisper_port
                ));
            }
            ServiceStatus::LoadingModel => {
                let _ = log_tx.send(format!(
                    "[Health Check] Whisper STT (port {}): Initializing model in VRAM...",
                    cfg.whisper_port
                ));
            }
            ServiceStatus::Offline {
                container_status: Some(s),
            } => {
                let _ = log_tx.send(format!(
                    "[Health Check] [Warning] Whisper port {} OFFLINE. Container status: '{}'.",
                    cfg.whisper_port, s
                ));
            }
            ServiceStatus::Offline {
                container_status: None,
            } => {
                let _ = log_tx.send(format!(
                    "[Health Check] [Warning] Whisper endpoint OFFLINE on port {}.",
                    cfg.whisper_port
                ));
            }
        }

        // 4. Qwen LLM
        match check_service_endpoint(
            &client,
            cfg.qwen_port,
            &cfg.qwen_container_name,
            podman_available,
        ) {
            ServiceStatus::Online => {
                let _ = log_tx.send(format!(
                    "[Health Check] [Success] Qwen LLM (port {}): ONLINE and ready.",
                    cfg.qwen_port
                ));
            }
            ServiceStatus::LoadingModel => {
                let _ = log_tx.send(format!(
                    "[Health Check] Qwen LLM (port {}): Initializing model...",
                    cfg.qwen_port
                ));
            }
            ServiceStatus::Offline { .. } => {
                let _ = log_tx.send(format!(
                    "[Health Check] Qwen LLM endpoint OFFLINE on port {}. (Optional for summaries/spellcheck).",
                    cfg.qwen_port
                ));
            }
        }
    });
}
