// src/ui/config.rs
use crate::config_struct::AppConfig;
use crate::podman::manager;
use eframe::egui;
use std::sync::mpsc::Sender;

pub fn show(config: &mut AppConfig, log_tx: &Sender<String>, ui: &mut egui::Ui) {
    let mut changed = false;

    egui::ScrollArea::vertical()
        .id_salt("config_scroll_area")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.heading("Settings & Configuration");
            ui.add_space(10.0);

            // 1. General Audio & Subtitle Defaults
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

                    ui.label("Whisper Chunk Size (0-300s):");
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::DragValue::new(&mut config.whisper_chunk_duration_secs)
                                    .range(0..=300)
                                    .suffix(" secs"),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                        ui.label(
                            egui::RichText::new("(0 = full file, up to 300 secs / 5 min)")
                                .weak()
                                .small(),
                        );
                    });
                    ui.end_row();

                    ui.label("Max characters per line:");
                    if ui
                        .add(
                            egui::Slider::new(&mut config.max_chars_per_line, 10..=80)
                                .text("chars"),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Subtitle gap:");
                    if ui
                        .add(
                            egui::DragValue::new(&mut config.subtitle_gap_ms)
                                .speed(10.0)
                                .suffix(" ms"),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                    ui.end_row();
                });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // 2. Whisper Model Settings
            ui.heading("Whisper Model (Speech-to-Text)");
            ui.add_space(4.0);

            egui::Grid::new("whisper_config_grid")
                .num_columns(2)
                .spacing([20.0, 12.0])
                .show(ui, |ui| {
                    ui.label("Whisper Port:");
                    ui.vertical(|ui| {
                        if ui
                            .add(egui::DragValue::new(&mut config.whisper_port).range(1024..=65535))
                            .changed()
                        {
                            changed = true;
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "Inference URL: http://127.0.0.1:{}/inference",
                                config.whisper_port
                            ))
                            .weak()
                            .small(),
                        );
                    });
                    ui.end_row();
                });

            ui.add_space(6.0);
            if ui
                .checkbox(
                    &mut config.manage_whisper_container,
                    "Manage local Whisper container",
                )
                .changed()
            {
                changed = true;
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Container name:");
                let response = ui.add_enabled(
                    config.manage_whisper_container,
                    egui::TextEdit::singleline(&mut config.whisper_container_name)
                        .hint_text("whisper-server"),
                );
                if response.changed() {
                    changed = true;
                }

                if config.manage_whisper_container {
                    if ui.button("▶ Start").clicked() {
                        manager::start_container(
                            &config.whisper_container_name,
                            Some(log_tx.clone()),
                        );
                    }
                    if ui.button("⏹ Stop").clicked() {
                        manager::stop_container(
                            &config.whisper_container_name,
                            Some(log_tx.clone()),
                        );
                    }
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // 3. Qwen Model Settings
            ui.heading("Qwen 2.5 Coder (Notes & Summarization)");
            ui.add_space(4.0);

            egui::Grid::new("qwen_config_grid")
                .num_columns(2)
                .spacing([20.0, 12.0])
                .show(ui, |ui| {
                    ui.label("Qwen LLM Port:");
                    ui.vertical(|ui| {
                        if ui
                            .add(egui::DragValue::new(&mut config.qwen_port).range(1024..=65535))
                            .changed()
                        {
                            changed = true;
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "Endpoint: http://127.0.0.1:{}/v1/chat/completions",
                                config.qwen_port
                            ))
                            .weak()
                            .small(),
                        );
                    });
                    ui.end_row();
                });

            ui.add_space(6.0);
            if ui
                .checkbox(
                    &mut config.manage_qwen_container,
                    "Manage local Qwen container",
                )
                .changed()
            {
                changed = true;
            }

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Container name:");
                let response = ui.add_enabled(
                    config.manage_qwen_container,
                    egui::TextEdit::singleline(&mut config.qwen_container_name)
                        .hint_text("qwen-coder"),
                );
                if response.changed() {
                    changed = true;
                }

                if config.manage_qwen_container {
                    if ui.button("▶ Start").clicked() {
                        manager::start_container(&config.qwen_container_name, Some(log_tx.clone()));
                    }
                    if ui.button("⏹ Stop").clicked() {
                        manager::stop_container(&config.qwen_container_name, Some(log_tx.clone()));
                    }
                }
            });

            ui.add_space(16.0);
            if ui.button("🔄 Check All Services Status").clicked() {
                manager::check_services(config, log_tx.clone(), 0);
            }

            ui.add_space(20.0);
        });

    if changed {
        let _ = config.save();
    }
}
