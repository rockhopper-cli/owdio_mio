use crate::config::AppConfig;
use crate::pipelines::content::{GeneratedContent, run_content_generation};
use crate::subtitles::parser::clean_srt_for_llm;
use crate::ui::async_job::AsyncJob;
use eframe::egui;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

pub struct ContentCreatorState {
    pub file_path: Option<String>,
    pub parsed_transcript: String,
    pub generate_youtube: bool,
    pub generate_patreon: bool,
    pub result: Option<GeneratedContent>,

    pub job: AsyncJob<GeneratedContent>,
}

impl Default for ContentCreatorState {
    fn default() -> Self {
        Self {
            file_path: None,
            parsed_transcript: String::new(),
            generate_youtube: true,
            generate_patreon: true,
            result: None,
            job: AsyncJob::new(),
        }
    }
}

impl ContentCreatorState {
    pub fn poll(&mut self, log_tx: &Sender<String>) {
        let result_slot = &mut self.result;
        self.job.poll(log_tx, "Content Generator", |content| {
            *result_slot = Some(content);
        });
    }
}

pub fn show(
    state: &mut ContentCreatorState,
    config: &AppConfig,
    log_tx: &Sender<String>,
    ui: &mut egui::Ui,
) {
    state.poll(log_tx);

    egui::ScrollArea::vertical()
        .id_salt("content_creator_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.add_space(5.0);

            let heading_color_3 = if ui.visuals().dark_mode {
                egui::Color32::YELLOW
            } else {
                egui::Color32::DARK_BLUE // Warm dark amber for light mode
            };

            ui.heading(
                egui::RichText::new("YouTube & Patreon Content Generator")
                    .color(heading_color_3)
                    .strong()
                    .size(28.0),
            );
            ui.label(
                egui::RichText::new("Upload a verified/edited SRT subtitle or transcript file. Qwen Coder will extract chapter markers and compose your video copy.")
                    .italics(),
            );
            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);

            // 1. File Picker for SRT / TXT / VTT
            ui.horizontal(|ui| {
                let btn = ui.add_enabled(
                    !state.job.is_processing,
                    egui::Button::new("📄 Select .SRT or .TXT File"),
                );
                if btn.clicked() {
                    let mut dialog = rfd::FileDialog::new()
                        .add_filter("Subtitle / Transcript", &["srt", "txt", "vtt"]);

                    if config.wav_default_path.exists() {
                        dialog = dialog.set_directory(&config.wav_default_path);
                    }

                    if let Some(path) = dialog.pick_file() {
                        let path_str = path.display().to_string();
                        state.file_path = Some(path_str.clone());

                        match fs::read_to_string(&path) {
                            Ok(raw) => {
                                let is_subtitle = path.extension().is_some_and(|ext| {
                                    ext.eq_ignore_ascii_case("srt") || ext.eq_ignore_ascii_case("vtt")
                                });

                                if is_subtitle {
                                    state.parsed_transcript = clean_srt_for_llm(&raw);
                                    let _ = log_tx.send(format!("[Content Generator] Loaded & parsed subtitles: {}", path_str));
                                } else {
                                    state.parsed_transcript = raw;
                                    let _ = log_tx.send(format!("[Content Generator] Loaded transcript: {}", path_str));
                                }
                            }
                            Err(e) => {
                                let _ = log_tx.send(format!("[Content Generator] Error reading file '{}': {}", path_str, e));
                            }
                        }
                    }
                }

                if let Some(ref path) = state.file_path {
                    ui.label(path.as_str());
                } else {
                    ui.label(egui::RichText::new("No file selected.").color(egui::Color32::RED));
                }
            });

            if !state.parsed_transcript.is_empty() {
                ui.add_space(6.0);
                egui::CollapsingHeader::new(format!(
                    "Preview Parsed Transcript ({} characters)",
                    state.parsed_transcript.len()
                ))
                .default_open(false)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            ui.monospace(&state.parsed_transcript);
                        });
                });
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 2. Options
            ui.label(egui::RichText::new("Generation Options:").strong());
            ui.add_space(4.0);
            ui.add_enabled(
                !state.job.is_processing,
                egui::Checkbox::new(&mut state.generate_youtube, "YouTube Description (with Chapters & Timestamps)"),
            );
            ui.add_space(4.0);
            ui.add_enabled(
                !state.job.is_processing,
                egui::Checkbox::new(&mut state.generate_patreon, "Patreon Community Post"),
            );

            ui.add_space(12.0);

            // 3. Action Buttons
            let can_start = !state.job.is_processing
                && !state.parsed_transcript.is_empty()
                && (state.generate_youtube || state.generate_patreon);

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        can_start,
                        egui::Button::new(if state.job.is_processing {
                            "Generating with Qwen..."
                        } else {
                            "✨ Generate Content"
                        }),
                    )
                    .clicked()
                {
                    state.result = None;
                    let _ = log_tx.send("[Content Generator] Starting Qwen generation...".into());

                    let qwen_port = config.qwen_port;
                    let transcript = state.parsed_transcript.clone();
                    let input_path = state.file_path.clone().map(PathBuf::from);
                    let gen_yt = state.generate_youtube;
                    let gen_patreon = state.generate_patreon;

                    state.job.spawn("Connecting to Qwen...", move |cancel_token, tx| {
                        run_content_generation(
                            transcript,
                            input_path,
                            qwen_port,
                            gen_yt,
                            gen_patreon,
                            cancel_token,
                            tx,
                        );
                    });
                }

                if state.job.is_processing && ui.button("⏹ Cancel").clicked() {
                    state.job.cancel();
                }

                if state.job.is_processing {
                    ui.spinner();
                    ui.label(&state.job.progress_text);
                }
            });

            ui.add_space(15.0);

            // 4. Results Display
            if let Some(ref res) = state.result
                && (res.youtube.is_some() || res.patreon.is_some()) {
                    ui.separator();
                    ui.heading("Generated Copy");

                    if let Some((ref yt_text, ref yt_path)) = res.youtube {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("📺 YouTube Description & Chapters:").strong());
                            if ui.button("📋 Copy").clicked() {
                                ui.ctx().copy_text(yt_text.clone());
                            }
                        });
                        if let Some(path) = yt_path {
                            ui.label(egui::RichText::new(format!("📁 Saved to: {}", path)).weak().small());
                        }
                        egui::ScrollArea::vertical()
                            .id_salt("content_yt_scroll")
                            .max_height(180.0)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(yt_text);
                            });
                    }

                    if let Some((ref patreon_text, ref patreon_path)) = res.patreon {
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("🧡 Patreon Community Post:").strong());
                            if ui.button("📋 Copy").clicked() {
                                ui.ctx().copy_text(patreon_text.clone());
                            }
                        });
                        if let Some(path) = patreon_path {
                            ui.label(egui::RichText::new(format!("📁 Saved to: {}", path)).weak().small());
                        }
                        egui::ScrollArea::vertical()
                            .id_salt("content_patreon_scroll")
                            .max_height(180.0)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(patreon_text);
                            });
                    }
                }
        });
}
