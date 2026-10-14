// src/ui/subtitle_creator.rs
use crate::audio_pipeline::PipelineMessage;
use crate::config_struct::AppConfig;
use crate::subtitle_pipeline::SubtitleOptions;
use crate::ui::components;
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

pub struct SubtitleState {
    pub wav_file_path: Option<String>,
    pub max_chars_per_line: usize,
    pub two_lines_subtitles: bool,
    pub min_gap_ms: u64,
    pub chunk_duration_secs: u32, // Stored directly as 0 to 300 seconds

    // UI Async Tracking
    pub is_processing: bool,
    pub progress: f32,
    pub progress_text: String,
    pub cancel_token: Arc<AtomicBool>,
    receiver: Option<Receiver<PipelineMessage<String>>>,
}

impl Default for SubtitleState {
    fn default() -> Self {
        Self {
            wav_file_path: None,
            max_chars_per_line: 42,
            two_lines_subtitles: true,
            min_gap_ms: 100,
            chunk_duration_secs: 60,
            is_processing: false,
            progress: 0.0,
            progress_text: String::from("Idle"),
            cancel_token: Arc::new(AtomicBool::new(false)),
            receiver: None,
        }
    }
}

impl SubtitleState {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            max_chars_per_line: config.max_chars_per_line,
            min_gap_ms: config.subtitle_gap_ms as u64,
            chunk_duration_secs: config.whisper_chunk_duration_secs,
            ..Default::default()
        }
    }

    pub fn poll(&mut self, log_tx: &Sender<String>) {
        if let Some(ref rx) = self.receiver {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    PipelineMessage::Log(line) => {
                        let _ = log_tx.send(line);
                    }
                    PipelineMessage::Progress { progress, text } => {
                        self.progress = progress;
                        self.progress_text = text;
                    }
                    PipelineMessage::Finished(result) => {
                        self.is_processing = false;
                        match result {
                            Ok(srt_path) => {
                                self.progress = 1.0;
                                self.progress_text = "Completed".to_string();
                                let _ = log_tx.send(format!(
                                    "[Subtitles] Successfully saved SRT to: {}",
                                    srt_path
                                ));
                            }
                            Err(err) => {
                                self.progress = 0.0;
                                self.progress_text = "Failed".to_string();
                                let _ = log_tx.send(format!("[Subtitles] Error: {}", err));
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn show(
    state: &mut SubtitleState,
    config: &AppConfig,
    log_tx: &Sender<String>,
    ui: &mut egui::Ui,
) {
    ui.add_space(5.0);

    ui.heading(
        egui::RichText::new("Subtitle Creator")
            .color(egui::Color32::ORANGE)
            .strong()
            .size(28.0),
    );

    ui.label(
        egui::RichText::new("Generate non-overlapping, formatted subtitles using Whisper.")
            .color(egui::Color32::from_rgb(180, 180, 180))
            .italics(),
    );
    ui.add_space(15.0);
    ui.separator();
    ui.add_space(8.0);

    // 1. Media File Picker
    components::audio_file_picker(
        ui,
        &mut state.wav_file_path,
        &config.wav_default_path,
        state.is_processing,
    );
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // 2. Chunking & Formatting Controls
    ui.horizontal(|ui| {
        ui.label("Audio chunk size (0 - 300 secs):  ");
        ui.add_enabled(
            !state.is_processing,
            egui::DragValue::new(&mut state.chunk_duration_secs)
                .range(0..=300)
                .suffix(" secs"),
        );

        ui.label(
            egui::RichText::new("(0 = process entire file, up to 300s / 5 min)")
                .weak()
                .italics()
                .size(11.0),
        );
    });
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.label("Max characters per line (max 80):");
        ui.add_enabled(
            !state.is_processing,
            egui::DragValue::new(&mut state.max_chars_per_line).range(10..=80),
        );
    });
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.label("Gap between subtitles:                 ");
        ui.add_enabled(
            !state.is_processing,
            egui::DragValue::new(&mut state.min_gap_ms)
                .range(0..=500)
                .suffix(" ms"),
        );

        ui.label(
            egui::RichText::new("(prevents subtitles from sticking/overlapping)")
                .weak()
                .italics()
                .size(11.0),
        );
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Limit to 2 lines per subtitle:           ");
        ui.add_enabled(
            !state.is_processing,
            egui::Checkbox::new(&mut state.two_lines_subtitles, ""),
        );
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    if !state.is_processing && ui.button("↺ Reset to Config Defaults").clicked() {
        state.max_chars_per_line = config.max_chars_per_line;
        state.min_gap_ms = config.subtitle_gap_ms as u64;
        state.chunk_duration_secs = config.whisper_chunk_duration_secs;
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // 3. Action Buttons (Generate + Cancel)
    let can_start = !state.is_processing && state.wav_file_path.is_some();
    ui.horizontal(|ui| {
        let generate_btn = ui.add_enabled(
            can_start,
            egui::Button::new(if state.is_processing {
                "Processing..."
            } else {
                "Generate Subtitles"
            }),
        );

        if generate_btn.clicked()
            && let Some(wav_path) = state.wav_file_path.clone()
        {
            state.is_processing = true;
            state.progress = 0.05;
            state.progress_text = "Starting...".to_string();
            state.cancel_token = Arc::new(AtomicBool::new(false)); // Reset token
            let _ = log_tx.send(format!(
                "--- Starting Subtitle Generation for: {} ---",
                wav_path
            ));

            let (tx, rx): (
                Sender<PipelineMessage<String>>,
                Receiver<PipelineMessage<String>>,
            ) = channel();
            state.receiver = Some(rx);

            let options = SubtitleOptions {
                chunk_duration_secs: state.chunk_duration_secs,
                max_chars_per_line: state.max_chars_per_line,
                two_lines: state.two_lines_subtitles,
                min_gap_ms: state.min_gap_ms,
            };
            let inference_url = format!("http://127.0.0.1:{}/inference", config.whisper_port);
            let cancel_token = state.cancel_token.clone();

            thread::spawn(move || {
                let result =
                    run_pipeline_task(wav_path, inference_url, options, cancel_token, tx.clone());
                let _ = tx.send(PipelineMessage::Finished(result));
            });
        }

        if state.is_processing && ui.button("⏹ Cancel").clicked() {
            state.cancel_token.store(true, Ordering::Relaxed);
            state.progress_text = "Cancelling...".to_string();
        }
    });

    ui.add_space(10.0);

    // 4. Progress Bar
    components::render_progress_bar(
        ui,
        state.progress,
        &state.progress_text,
        state.is_processing,
    );
}

fn run_pipeline_task(
    wav_path: String,
    inference_url: String,
    options: SubtitleOptions,
    cancel_token: Arc<AtomicBool>,
    tx: Sender<PipelineMessage<String>>,
) -> Result<String, String> {
    let srt = crate::subtitle_pipeline::generate_subtitles(
        &wav_path,
        &inference_url,
        options,
        cancel_token,
        tx,
    )
    .map_err(|e| format!("{:#}", e))?;

    let srt_path = std::path::Path::new(&wav_path).with_extension("srt");
    std::fs::write(&srt_path, srt).map_err(|e| format!("Failed to save SRT file: {}", e))?;

    Ok(srt_path.display().to_string())
}
