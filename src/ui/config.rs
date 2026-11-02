use crate::config::AppConfig;
use crate::container::podman;
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

            ui.label(egui::RichText::new("Display & Audio Defaults").strong());
            ui.add_space(4.0);

            egui::Grid::new("general_config_grid")
                .num_columns(2)
                .spacing([20.0, 12.0])
                .show(ui, |ui| {
                    ui.label("UI Scale / Zoom:");
                    ui.horizontal(|ui| {
                        let slider = ui.add(
                            egui::Slider::new(&mut config.ui_zoom, 0.8..=2.5)
                                .step_by(0.05)
                                .suffix("x"),
                        );
                        if slider.changed() {
                            ui.ctx().set_zoom_factor(config.ui_zoom);
                        }
                        if slider.drag_stopped() {
                            changed = true;
                        }
                        if ui.button("↺ Reset (1.2x)").clicked() {
                            config.ui_zoom = 1.20;
                            ui.ctx().set_zoom_factor(config.ui_zoom);
                            changed = true;
                        }
                    });
                    ui.end_row();

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
                        let r = ui.add(
                            egui::DragValue::new(&mut config.whisper_chunk_duration_secs)
                                .range(0..=300)
                                .suffix(" secs"),
                        );
                        if r.drag_stopped() || r.lost_focus() {
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
                    let r_max_chars = ui.add(
                        egui::Slider::new(&mut config.max_chars_per_line, 10..=80).text("chars"),
                    );
                    if r_max_chars.drag_stopped() {
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Subtitle gap:");
                    let r_gap = ui.add(
                        egui::DragValue::new(&mut config.subtitle_gap_ms)
                            .speed(10.0)
                            .suffix(" ms"),
                    );
                    if r_gap.drag_stopped() || r_gap.lost_focus() {
                        changed = true;
                    }
                    ui.end_row();
                });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            ui.heading("Whisper Model (Speech-to-Text)");
            ui.add_space(4.0);

            egui::Grid::new("whisper_config_grid")
                .num_columns(2)
                .spacing([20.0, 12.0])
                .show(ui, |ui| {
                    ui.label("Whisper Port:");
                    ui.vertical(|ui| {
                        let r_port = ui.add(
                            egui::DragValue::new(&mut config.whisper_port).range(1024..=65535),
                        );
                        if r_port.drag_stopped() || r_port.lost_focus() {
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
                if response.lost_focus() {
                    changed = true;
                }

                if config.manage_whisper_container {
                    if ui.button("▶ Start").clicked() {
                        podman::start_container(
                            &config.whisper_container_name,
                            Some(log_tx.clone()),
                        );
                    }
                    if ui.button("⏹ Stop").clicked() {
                        podman::stop_container(
                            &config.whisper_container_name,
                            Some(log_tx.clone()),
                        );
                    }
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            ui.heading("Qwen 2.5 Coder (Notes, Proofreading & Summarization)");
            ui.add_space(4.0);

            egui::Grid::new("qwen_config_grid")
                .num_columns(2)
                .spacing([20.0, 12.0])
                .show(ui, |ui| {
                    ui.label("Qwen LLM Port:");
                    ui.vertical(|ui| {
                        let r_qwen_port =
                            ui.add(egui::DragValue::new(&mut config.qwen_port).range(1024..=65535));
                        if r_qwen_port.drag_stopped() || r_qwen_port.lost_focus() {
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
                if response.lost_focus() {
                    changed = true;
                }

                if config.manage_qwen_container {
                    if ui.button("▶ Start").clicked() {
                        podman::start_container(&config.qwen_container_name, Some(log_tx.clone()));
                    }
                    if ui.button("⏹ Stop").clicked() {
                        podman::stop_container(&config.qwen_container_name, Some(log_tx.clone()));
                    }
                }
            });

            ui.add_space(16.0);
            if ui.button("🔄 Check All Services Status").clicked() {
                podman::check_services(config, log_tx.clone(), 0);
            }

            ui.add_space(20.0);
        });

    if changed {
        let _ = config.save();
    }
}
