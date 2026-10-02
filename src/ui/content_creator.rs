// src/ui/content_creator.rs
use crate::audio_pipeline::PipelineMessage;
use crate::podman::llm_client::LlmClient;
use crate::ui::config::AppConfig;
use eframe::egui;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

#[derive(Clone, Debug, Default)]
pub struct GeneratedContent {
    pub youtube: Option<(String, Option<String>)>, // (text, optional saved_path)
    pub patreon: Option<(String, Option<String>)>, // (text, optional saved_path)
}

pub struct ContentCreatorState {
    pub file_path: Option<String>,
    pub parsed_transcript: String,
    pub generate_youtube: bool,
    pub generate_patreon: bool,

    pub is_processing: bool,
    pub progress_text: String,
    pub result: Option<GeneratedContent>,

    pub cancel_token: Arc<AtomicBool>,
    receiver: Option<Receiver<PipelineMessage<GeneratedContent>>>,
}

impl Default for ContentCreatorState {
    fn default() -> Self {
        Self {
            file_path: None,
            parsed_transcript: String::new(),
            generate_youtube: true,
            generate_patreon: true,
            is_processing: false,
            progress_text: "Idle".to_string(),
            result: None,
            cancel_token: Arc::new(AtomicBool::new(false)),
            receiver: None,
        }
    }
}

impl ContentCreatorState {
    pub fn poll(&mut self, log_tx: &Sender<String>) {
        if let Some(ref rx) = self.receiver {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    PipelineMessage::Log(line) => {
                        let _ = log_tx.send(line);
                    }
                    PipelineMessage::Progress { text, .. } => {
                        self.progress_text = text;
                    }
                    PipelineMessage::Finished(res) => {
                        self.is_processing = false;
                        match res {
                            Ok(content) => {
                                self.progress_text = "Completed".to_string();
                                let _ = log_tx.send(
                                    "[Content Generator] Content generated successfully!".into(),
                                );
                                self.result = Some(content);
                            }
                            Err(err) => {
                                self.progress_text = "Failed".to_string();
                                let _ = log_tx.send(format!("[Content Generator] Error: {}", err));
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Parses an SRT/VTT file into clean `[MM:SS] Text` lines for the LLM without data loss
fn clean_srt_for_llm(content: &str) -> String {
    let mut cleaned = String::with_capacity(content.len());
    let mut current_timestamp = String::new();
    let mut current_text = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            // End of cue: flush accumulated text with its timestamp
            if !current_timestamp.is_empty() && !current_text.is_empty() {
                let _ = writeln!(
                    cleaned,
                    "[{}] {}",
                    current_timestamp,
                    current_text.join(" ")
                );
                current_timestamp.clear();
                current_text.clear();
            }
            continue;
        }

        if line.contains("-->") {
            // Flush any un-flushed previous cue (handles malformed SRT without blank lines)
            if !current_timestamp.is_empty() && !current_text.is_empty() {
                let _ = writeln!(
                    cleaned,
                    "[{}] {}",
                    current_timestamp,
                    current_text.join(" ")
                );
                current_text.clear();
            }

            // Parse cue start timestamp: "00:01:23,456 --> 00:01:28,000" (or VTT with '.')
            if let Some(start_time) = line.split("-->").next() {
                let clean_time = start_time.trim().replace('.', ",");
                let parts: Vec<&str> = clean_time.split(':').collect();
                if parts.len() == 3 {
                    let h: u32 = parts[0].parse().unwrap_or(0);
                    let m = parts[1];
                    let s = parts[2].split(',').next().unwrap_or("00");

                    current_timestamp = if h > 0 {
                        format!("{:02}:{}:{}", h, m, s)
                    } else {
                        format!("{}:{}", m, s)
                    };
                }
            }
        } else if !current_timestamp.is_empty() {
            // Inside cue text (preserves all lines within the cue)
            current_text.push(line);
        } else if !line.chars().all(|c| c.is_ascii_digit()) && line != "WEBVTT" {
            // Plain transcript line
            let _ = writeln!(cleaned, "{}", line);
        }
    }

    // Flush trailing cue at EOF
    if !current_timestamp.is_empty() && !current_text.is_empty() {
        let _ = writeln!(
            cleaned,
            "[{}] {}",
            current_timestamp,
            current_text.join(" ")
        );
    }

    if cleaned.is_empty() {
        content.to_string()
    } else {
        cleaned
    }
}

pub fn show(
    state: &mut ContentCreatorState,
    config: &AppConfig,
    log_tx: &Sender<String>,
    ui: &mut egui::Ui,
) {
    state.poll(log_tx);

    if state.is_processing {
        ui.ctx().request_repaint();
    }

    egui::ScrollArea::vertical()
        .id_salt("content_creator_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.add_space(5.0);
            ui.heading(
                egui::RichText::new("YouTube & Patreon Content Generator")
                    .color(egui::Color32::YELLOW)
                    .strong()
                    .size(26.0),
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
                    !state.is_processing,
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
                                // Case-insensitive extension check
                                let is_subtitle = path.extension().map_or(false, |ext| {
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

            // Optional preview of parsed text
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

            // 2. Options (Cleaned of redundant ui.horizontal)
            ui.label(egui::RichText::new("Generation Options:").strong());
            ui.add_space(4.0);
            ui.add_enabled(
                !state.is_processing,
                egui::Checkbox::new(&mut state.generate_youtube, "YouTube Description (with Chapters & Timestamps)"),
            );
            ui.add_space(4.0);
            ui.add_enabled(
                !state.is_processing,
                egui::Checkbox::new(&mut state.generate_patreon, "Patreon Community Post"),
            );

            ui.add_space(12.0);

            // 3. Action Buttons
            let can_start = !state.is_processing
                && !state.parsed_transcript.is_empty()
                && (state.generate_youtube || state.generate_patreon);

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        can_start,
                        egui::Button::new(if state.is_processing {
                            "Generating with Qwen..."
                        } else {
                            "✨ Generate Content"
                        }),
                    )
                    .clicked()
                {
                    state.is_processing = true;
                    state.result = None; // Reset previous results on new run
                    state.progress_text = "Connecting to Qwen...".to_string();
                    state.cancel_token = Arc::new(AtomicBool::new(false));
                    let cancel_token = state.cancel_token.clone();

                    let (tx, rx) = channel();
                    state.receiver = Some(rx);

                    let qwen_port = config.qwen_port;
                    let transcript = state.parsed_transcript.clone();
                    let input_path = state.file_path.clone().map(PathBuf::from);
                    let gen_yt = state.generate_youtube;
                    let gen_patreon = state.generate_patreon;

                    let _ = log_tx.send("[Content Generator] Starting Qwen generation...".into());

                    thread::spawn(move || {
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

                if state.is_processing && ui.button("⏹ Cancel").clicked() {
                    state.cancel_token.store(true, Ordering::Relaxed);
                    state.progress_text = "Cancelling...".to_string();
                }

                if state.is_processing {
                    ui.spinner();
                    ui.label(&state.progress_text);
                }
            });

            ui.add_space(15.0);

            // 4. Results Display
            if let Some(ref res) = state.result {
                if res.youtube.is_some() || res.patreon.is_some() {
                    ui.separator();
                    ui.heading("Generated Copy");

                    // --- YouTube Description ---
                    if let Some((ref yt_text, ref yt_path)) = res.youtube {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("📺 YouTube Description & Chapters:").strong());
                            if ui.button("📋 Copy").clicked() {
                                ui.ctx().copy_text(yt_text.clone());
                            }
                        });
                        if let Some( path) = yt_path {
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

                    // --- Patreon Post ---
                    if let Some((ref patreon_text, ref patreon_path)) = res.patreon {
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("🧡 Patreon Community Post:").strong());
                            if ui.button("📋 Copy").clicked() {
                                ui.ctx().copy_text(patreon_text.clone());
                            }
                        });
                        if let Some( path) = patreon_path {
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
            }
        });
}

fn run_content_generation(
    transcript: String,
    input_path: Option<PathBuf>,
    qwen_port: u16,
    gen_youtube: bool,
    gen_patreon: bool,
    cancel_token: Arc<AtomicBool>,
    tx: Sender<PipelineMessage<GeneratedContent>>,
) {
    let llm = LlmClient::new(qwen_port);
    let mut result = GeneratedContent::default();

    // 1. YouTube Description Generation
    if gen_youtube {
        if cancel_token.load(Ordering::Relaxed) {
            let _ = tx.send(PipelineMessage::Finished(Err("Cancelled by user".into())));
            return;
        }

        let _ = tx.send(PipelineMessage::Progress {
            progress: 0.5,
            text: "Generating YouTube chapters & description...".into(),
        });
        let _ = tx.send(PipelineMessage::Log(
            "[Content Generator] Querying Qwen for YouTube copy...".into(),
        ));

        let prompt = "You are an expert YouTube producer and SEO copywriter. \
                      Given the timestamped transcript from an SRT, produce an engaging YouTube video description.\n\
                      Structure requirements:\n\
                      1. Hook & Overview: 2-3 engaging sentences describing the video.\n\
                      2. Key Takeaways: 3-5 bullet points of what viewers will learn.\n\
                      3. TIMESTAMPS / CHAPTERS:\n\
                         - Identify natural topic transitions from the transcript timestamps.\n\
                         - The first chapter MUST start at '00:00 - Introduction' (or relevant title).\n\
                         - Format each timestamp strictly as 'MM:SS - Chapter Title' (or 'HH:MM:SS - Title').\n\
                      4. Call to Action: Remind viewers to like, subscribe, and comment.\n\
                      5. 5-8 relevant hashtags at the very bottom.";

        match llm.complete(prompt, &transcript) {
            Ok(yt) => {
                let path_str = input_path.as_ref().map(|p| {
                    let out_path = p.with_extension("youtube.txt");
                    let _ = fs::write(&out_path, &yt);
                    out_path.display().to_string()
                });
                result.youtube = Some((yt, path_str));
            }
            Err(e) => {
                let _ = tx.send(PipelineMessage::Finished(Err(format!(
                    "YouTube generation failed: {:#}",
                    e
                ))));
                return;
            }
        }
    }

    // 2. Patreon Community Post Generation
    if gen_patreon {
        if cancel_token.load(Ordering::Relaxed) {
            let _ = tx.send(PipelineMessage::Finished(Err("Cancelled by user".into())));
            return;
        }

        let _ = tx.send(PipelineMessage::Progress {
            progress: 0.9,
            text: "Generating Patreon community post...".into(),
        });
        let _ = tx.send(PipelineMessage::Log(
            "[Content Generator] Querying Qwen for Patreon post...".into(),
        ));

        let prompt = "You are a creator writing directly to your patrons/supporters.\n\
                      Given the transcript, write a warm, engaging Patreon post.\n\
                      Include:\n\
                      1. A warm personal greeting and thank you to supporters.\n\
                      2. What this episode/video is about, including exclusive creator insights.\n\
                      3. 1-2 open discussion questions to encourage patrons to leave comments below.";

        match llm.complete(prompt, &transcript) {
            Ok(patreon) => {
                let path_str = input_path.as_ref().map(|p| {
                    let out_path = p.with_extension("patreon.txt");
                    let _ = fs::write(&out_path, &patreon);
                    out_path.display().to_string()
                });
                result.patreon = Some((patreon, path_str));
            }
            Err(e) => {
                let _ = tx.send(PipelineMessage::Finished(Err(format!(
                    "Patreon generation failed: {:#}",
                    e
                ))));
                return;
            }
        }
    }

    let _ = tx.send(PipelineMessage::Finished(Ok(result)));
}
