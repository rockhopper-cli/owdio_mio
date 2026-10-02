// src/ui/transcription.rs
use crate::audio_pipeline::{self,LlmGenerationOptions, PipelineMessage, PipelineResult};
use crate::ui::components;
use crate::ui::config::AppConfig;

use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

pub struct TranscriptionState {
    pub wav_file_path: Option<String>,
    pub generate_summary: bool,

    pub is_processing: bool,
    pub progress: f32,
    pub progress_text: String,

    pub result: Option<PipelineResult>,
    pub cancel_token: Arc<AtomicBool>,
    receiver: Option<Receiver<PipelineMessage<PipelineResult>>>,
}

impl Default for TranscriptionState {
    fn default() -> Self {
        Self {
            wav_file_path: None,
            generate_summary: false,
            is_processing: false,
            progress: 0.0,
            progress_text: "Idle".to_string(),
            result: None,
            cancel_token: Arc::new(AtomicBool::new(false)),
            receiver: None,
        }
    }
}

impl TranscriptionState {
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
                    PipelineMessage::Finished(res) => {
                        self.is_processing = false;
                        match res {
                            Ok(res_data) => {
                                self.progress = 1.0;
                                self.progress_text = "Completed".to_string();
                                let _ = log_tx
                                    .send("[Transcription] Job completed successfully!".into());
                                self.result = Some(res_data);
                            }
                            Err(err) => {
                                self.progress = 0.0;
                                self.progress_text = "Failed".to_string();
                                let _ = log_tx.send(format!("[Transcription] Error: {}", err));
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn show(
    state: &mut TranscriptionState,
    config: &AppConfig,
    log_tx: &Sender<String>,
    ui: &mut egui::Ui,
) {
    egui::ScrollArea::vertical()
        .id_salt("transcription_main_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            // Header
            ui.add_space(5.0);
            ui.heading(
                egui::RichText::new("Audio Transcription & Content Creator")
                    .color(egui::Color32::LIGHT_GREEN)
                    .strong()
                    .size(28.0),
            );
            ui.label("Transcribe audio with Whisper and generate descriptions, chapters, and notes with Qwen Coder.");
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

            // 2. AI Post-Processing Options
            ui.label(egui::RichText::new("AI Content Generation (Qwen 2.5 Coder):").strong());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_enabled(
                    !state.is_processing,
                    egui::Checkbox::new(&mut state.generate_summary, "Executive Summary"),
                );
            });
 
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // 3. Action Button
            let can_start = !state.is_processing && state.wav_file_path.is_some();
            if ui
                .add_enabled(
                    can_start,
                    egui::Button::new(if state.is_processing {
                        "Processing..."
                    } else {
                        "Start Processing"
                    }),
                )
                .clicked()
            {
                if let Some(wav_path) = state.wav_file_path.clone() {
                    state.is_processing = true;
                    state.progress = 0.05;
                    state.progress_text = "Starting...".into();
                    let _ =
                        log_tx.send(format!("--- Starting Pipeline for: {} ---", wav_path));
                    state.result = None;

                    let (tx, rx) = channel();
                    state.receiver = Some(rx);

                    let whisper_port = config.whisper_port;
                    let qwen_port = config.qwen_port;
                    let options = LlmGenerationOptions {
                        summary: state.generate_summary,
                        youtube: false,
                        patreon: false,
                    };
                    let cancel_token = state.cancel_token.clone();

                    thread::spawn(move || {
                        let res = audio_pipeline::run_audio_pipeline(
                            wav_path,
                            whisper_port,
                            qwen_port,
                            options,
                            cancel_token,
                            tx.clone(),
                        );
                        let _ = tx.send(PipelineMessage::Finished(res));
                    });
                }
            }
            ui.add_space(10.0);

            // 4. Progress Bar
            components::render_progress_bar(
                ui,
                state.progress,
                &state.progress_text,
                state.is_processing,
            );
            ui.add_space(10.0);

        // 5. Output Result Viewer
        if let Some(res) = &state.result {
            ui.add_space(12.0);
            ui.separator();
            ui.heading("Generated Results");

            // --- YouTube Description ---
            if let Some(yt) = &res.youtube_text {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("📺 YouTube Description & Chapters:").strong());
                    if ui.button("📋 Copy").clicked() {
                        ui.ctx().copy_text(yt.clone());
                    }
                });
                if let Some(path) = &res.youtube_path {
                    ui.label(egui::RichText::new(format!("📁 Saved to: {}", path)).weak().small());
                }
                egui::ScrollArea::vertical()
                    .id_salt("yt_scroll")
                    .max_height(140.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(yt);
                    });
            }

            // --- Patreon Post ---
            if let Some(patreon) = &res.patreon_text {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("🧡 Patreon Post:").strong());
                    if ui.button("📋 Copy").clicked() {
                        ui.ctx().copy_text(patreon.clone());
                    }
                });
                if let Some(path) = &res.patreon_path {
                    ui.label(egui::RichText::new(format!("📁 Saved to: {}", path)).weak().small());
                }
                egui::ScrollArea::vertical()
                    .id_salt("patreon_scroll")
                    .max_height(140.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(patreon);
                    });
            }

            // --- Summary ---
            if let Some(summary) = &res.summary_text {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("📝 Executive Summary:").strong());
                    if ui.button("📋 Copy").clicked() {
                        ui.ctx().copy_text(summary.clone());
                    }
                });
                if let Some(path) = &res.summary_path {
                    ui.label(egui::RichText::new(format!("📁 Saved to: {}", path)).weak().small());
                }
                egui::ScrollArea::vertical()
                    .id_salt("summary_scroll")
                    .max_height(140.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(summary);
                    });
            }

            // --- Full Transcript ---
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("📄 Full Timestamped Transcript:").strong());
                if ui.button("📋 Copy").clicked() {
                    ui.ctx().copy_text(res.transcript_text.clone());
                }
            });
            ui.label(
                egui::RichText::new(format!("📁 Saved to: {}", res.transcript_path))
                    .weak()
                    .small(),
            );
            egui::ScrollArea::vertical()
                .id_salt("transcript_scroll")
                .max_height(180.0)
                .show(ui, |ui| {
                    ui.label(&res.transcript_text);
                });
        }
    });
}
