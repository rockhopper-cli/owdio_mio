use crate::config::AppConfig;
use crate::pipelines::PipelineMessage;
use crate::pipelines::transcription::{self, LlmGenerationOptions, PipelineResult};
use crate::ui::async_job::AsyncJob;
use crate::ui::components;
use eframe::egui;
use std::sync::mpsc::Sender;

pub struct TranscriptionState {
    pub wav_file_path: Option<String>,
    pub generate_spellcheck: bool,
    pub generate_summary: bool,
    pub result: Option<PipelineResult>,

    pub job: AsyncJob<PipelineResult>,
}

impl Default for TranscriptionState {
    fn default() -> Self {
        Self {
            wav_file_path: None,
            generate_spellcheck: false,
            generate_summary: true,
            result: None,
            job: AsyncJob::new(),
        }
    }
}

impl TranscriptionState {
    pub fn poll(&mut self, log_tx: &Sender<String>) {
        let result_slot = &mut self.result;
        self.job.poll(log_tx, "Transcription", |res| {
            *result_slot = Some(res);
        });
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
            ui.add_space(5.0);

            let heading_color_2 = if ui.visuals().dark_mode {
                egui::Color32::LIGHT_GREEN
            } else {
                egui::Color32::DARK_GREEN
            };

            ui.heading(
                egui::RichText::new("Audio Transcription & AI Editor")
                    .color(heading_color_2)
                    .strong()
                    .size(28.0),
            );
            ui.label(
                "Transcribe audio locally using Whisper, with AI proofreading and executive summaries via Qwen Coder.",
            );
            ui.add_space(15.0);
            ui.separator();
            ui.add_space(8.0);

            // 1. File Picker
            components::audio_file_picker(
                ui,
                &mut state.wav_file_path,
                &config.wav_default_path,
                state.job.is_processing,
            );
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // 2. AI Options
            ui.label(egui::RichText::new("AI Content Generation (Qwen 2.5 Coder):").strong());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_enabled(
                    !state.job.is_processing,
                    egui::Checkbox::new(&mut state.generate_spellcheck, "✨ AI Spell Check & Proofreading"),
                );
                ui.add_space(16.0);
                ui.add_enabled(
                    !state.job.is_processing,
                    egui::Checkbox::new(&mut state.generate_summary, "📝 Executive Summary"),
                );
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // 3. Action Buttons
            let can_start = !state.job.is_processing && state.wav_file_path.is_some();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        can_start,
                        egui::Button::new(if state.job.is_processing {
                            "Processing..."
                        } else {
                            "Start Processing"
                        }),
                    )
                    .clicked()
                    && let Some(wav_path) = state.wav_file_path.clone()
                {
                    state.result = None;
                    let _ = log_tx.send(format!("--- Starting Pipeline for: {} ---", wav_path));

                    let whisper_port = config.whisper_port;
                    let qwen_port = config.qwen_port;
                    let chunk_duration_secs = config.whisper_chunk_duration_secs;
                    let options = LlmGenerationOptions {
                        spellcheck: state.generate_spellcheck,
                        summary: state.generate_summary,
                    };

                    state.job.spawn("Starting...", move |cancel_token, tx| {
                        let res = transcription::run_audio_pipeline(
                            wav_path,
                            whisper_port,
                            qwen_port,
                            chunk_duration_secs,
                            options,
                            cancel_token,
                            tx.clone(),
                        );
                        let _ = tx.send(PipelineMessage::Finished(res));
                    });
                }

                if state.job.is_processing && ui.button("⏹ Cancel").clicked() {
                    state.job.cancel();
                }
            });

            ui.add_space(10.0);

            // 4. Progress Bar
            components::render_progress_bar(
                ui,
                state.job.progress,
                &state.job.progress_text,
                state.job.is_processing,
            );
            ui.add_space(10.0);

            // 5. Results
            if let Some(res) = &state.result {
                ui.add_space(12.0);
                ui.separator();
                ui.heading("Generated Results");

                if let Some(proofread) = &res.proofread_text {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("✨ Proofread & Corrected Transcript:").strong().color(egui::Color32::LIGHT_BLUE));
                        if ui.button("📋 Copy").clicked() {
                            ui.ctx().copy_text(proofread.clone());
                        }
                    });
                    if let Some(path) = &res.proofread_path {
                        ui.label(egui::RichText::new(format!("📁 Saved to: {}", path)).weak().small());
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("proofread_scroll")
                        .max_height(160.0)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(proofread);
                        });
                }

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

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("📄 Original Timestamped Transcript:").strong());
                    if ui.button("📋 Copy").clicked() {
                        ui.ctx().copy_text(res.transcript_text.clone());
                    }
                });
                ui.label(egui::RichText::new(format!("📁 Saved to: {}", res.transcript_path)).weak().small());
                egui::ScrollArea::vertical()
                    .id_salt("raw_transcript_scroll")
                    .max_height(140.0)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(&res.transcript_text);
                    });
            }
        });
}
