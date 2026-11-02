use crate::config::AppConfig;
use crate::pipelines::PipelineMessage;
use crate::pipelines::subtitles::{SubtitleOptions, SubtitlePipelineResult, generate_subtitles};
use crate::ui::async_job::AsyncJob;
use crate::ui::components;
use eframe::egui;
use std::sync::mpsc::Sender;

pub struct SubtitleState {
    pub wav_file_path: Option<String>,
    pub max_chars_per_line: usize,
    pub two_lines_subtitles: bool,
    pub min_gap_ms: u64,
    pub chunk_duration_secs: u32,
    pub generate_spellcheck: bool,
    pub result: Option<SubtitlePipelineResult>,

    pub job: AsyncJob<SubtitlePipelineResult>,
}

impl Default for SubtitleState {
    fn default() -> Self {
        Self {
            wav_file_path: None,
            max_chars_per_line: 42,
            two_lines_subtitles: true,
            min_gap_ms: 100,
            chunk_duration_secs: 60,
            generate_spellcheck: false,
            result: None,
            job: AsyncJob::new(),
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
        let result_slot = &mut self.result;
        self.job.poll(log_tx, "Subtitles", |res| {
            *result_slot = Some(res);
        });
    }
}

pub fn show(
    state: &mut SubtitleState,
    config: &AppConfig,
    log_tx: &Sender<String>,
    ui: &mut egui::Ui,
) {
    ui.add_space(5.0);

    let heading_color_1 = if ui.visuals().dark_mode {
        egui::Color32::ORANGE
    } else {
        egui::Color32::DARK_RED
    };

    ui.heading(
        egui::RichText::new("Subtitle Creator")
            .color(heading_color_1)
            .strong()
            .size(28.0),
    );

    ui.label(
        egui::RichText::new("Generate non-overlapping, formatted subtitles using Whisper with optional Qwen AI proofreading.")
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
        state.job.is_processing,
    );
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // 2. Chunking & Formatting Controls
    ui.horizontal(|ui| {
        ui.label("Audio chunk size (0 - 300 secs):  ");
        ui.add_enabled(
            !state.job.is_processing,
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
            !state.job.is_processing,
            egui::DragValue::new(&mut state.max_chars_per_line).range(10..=80),
        );
    });
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.label("Gap between subtitles:                 ");
        ui.add_enabled(
            !state.job.is_processing,
            egui::DragValue::new(&mut state.min_gap_ms)
                .range(0..=500)
                .suffix(" ms"),
        );
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Limit to 2 lines per subtitle:           ");
        ui.add_enabled(
            !state.job.is_processing,
            egui::Checkbox::new(&mut state.two_lines_subtitles, ""),
        );
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_enabled(
            !state.job.is_processing,
            egui::Checkbox::new(
                &mut state.generate_spellcheck,
                "✨ AI Spell Check & Proofreading (Qwen 2.5)",
            ),
        );
    });

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // 3. Action Buttons (Generate + Cancel)
    let can_start = !state.job.is_processing && state.wav_file_path.is_some();
    ui.horizontal(|ui| {
        let generate_btn = ui.add_enabled(
            can_start,
            egui::Button::new(if state.job.is_processing {
                "Processing..."
            } else {
                "Generate Subtitles"
            }),
        );

        if generate_btn.clicked()
            && let Some(wav_path) = state.wav_file_path.clone()
        {
            state.result = None;
            let _ = log_tx.send(format!(
                "--- Starting Subtitle Generation for: {} ---",
                wav_path
            ));

            let options = SubtitleOptions {
                chunk_duration_secs: state.chunk_duration_secs,
                max_chars_per_line: state.max_chars_per_line,
                two_lines: state.two_lines_subtitles,
                min_gap_ms: state.min_gap_ms,
                spellcheck: state.generate_spellcheck,
                qwen_port: config.qwen_port,
            };
            let inference_url = format!("http://127.0.0.1:{}/inference", config.whisper_port);

            state.job.spawn("Starting...", move |cancel_token, tx| {
                let res =
                    generate_subtitles(wav_path, &inference_url, options, cancel_token, tx.clone())
                        .map_err(|e| format!("{:#}", e));
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

    // 5. Results Display
    if let Some(ref res) = state.result {
        ui.add_space(12.0);
        ui.separator();
        ui.heading("Generated Subtitles");

        if let Some(ref proofread_text) = res.proofread_srt {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("✨ Proofread Subtitles (.srt):")
                        .strong()
                        .color(egui::Color32::LIGHT_BLUE),
                );
                if ui.button("📋 Copy Proofread SRT").clicked() {
                    ui.ctx().copy_text(proofread_text.clone());
                }
            });
            if let Some(ref path) = res.proofread_srt_path {
                ui.label(
                    egui::RichText::new(format!("📁 Saved to: {}", path))
                        .weak()
                        .small(),
                );
            }
            egui::ScrollArea::vertical()
                .id_salt("proofread_srt_scroll")
                .max_height(140.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.monospace(proofread_text);
                });
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("📄 Original Subtitles (.srt):").strong());
            if ui.button("📋 Copy Original SRT").clicked() {
                ui.ctx().copy_text(res.raw_srt.clone());
            }
        });
        ui.label(
            egui::RichText::new(format!("📁 Saved to: {}", res.raw_srt_path))
                .weak()
                .small(),
        );
        egui::ScrollArea::vertical()
            .id_salt("raw_srt_scroll")
            .max_height(120.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.monospace(&res.raw_srt);
            });
    }
}
