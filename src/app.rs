use crate::config::AppConfig;
use crate::container::podman;
use crate::ui::{about, config, content_creator, setup_model, subtitle_creator, transcription};
use crate::utils::current_log_timestamp;
use eframe::egui;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;

#[derive(PartialEq)]
enum Tab {
    SubtitleCreator,
    AudioTranscription,
    ContentCreator,
    SetUpLocalModel,
    Configuration,
    About,
}

pub struct OwdioMio {
    current_tab: Tab,
    subtitle_state: subtitle_creator::SubtitleState,
    transcription_state: transcription::TranscriptionState,
    content_creator_state: content_creator::ContentCreatorState,
    pub config: AppConfig,

    pub logs: Vec<String>,
    pub log_sender: Sender<String>,
    pub log_receiver: Receiver<String>,
}

impl OwdioMio {
    pub fn new(loaded_config: AppConfig) -> Self {
        let (log_sender, log_receiver) = channel();

        if loaded_config.manage_whisper_container {
            podman::start_container(
                &loaded_config.whisper_container_name,
                Some(log_sender.clone()),
            );
        }
        if loaded_config.manage_qwen_container {
            podman::start_container(&loaded_config.qwen_container_name, Some(log_sender.clone()));
        }

        podman::check_services(&loaded_config, log_sender.clone(), 2500);

        Self {
            current_tab: Tab::SubtitleCreator,
            subtitle_state: subtitle_creator::SubtitleState::new(&loaded_config),
            transcription_state: transcription::TranscriptionState::default(),
            content_creator_state: content_creator::ContentCreatorState::default(),
            config: loaded_config,
            logs: vec!["[Owdio Mio] Application ready.".to_string()],
            log_sender,
            log_receiver,
        }
    }
}

impl Drop for OwdioMio {
    fn drop(&mut self) {
        let mut stop_handles = Vec::new();

        if self.config.manage_whisper_container {
            let name = self.config.whisper_container_name.clone();
            stop_handles.push(thread::spawn(move || {
                podman::stop_container_sync(&name);
            }));
        }
        if self.config.manage_qwen_container {
            let name = self.config.qwen_container_name.clone();
            stop_handles.push(thread::spawn(move || {
                podman::stop_container_sync(&name);
            }));
        }

        for handle in stop_handles {
            let _ = handle.join();
        }
    }
}

impl eframe::App for OwdioMio {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.input(|i| {
            if i.modifiers.command {
                if i.key_pressed(egui::Key::Equals) || i.key_pressed(egui::Key::Plus) {
                    self.config.ui_zoom = (self.config.ui_zoom + 0.1).min(2.5);
                    ui.ctx().set_zoom_factor(self.config.ui_zoom);
                    let _ = self.config.save();
                } else if i.key_pressed(egui::Key::Minus) {
                    self.config.ui_zoom = (self.config.ui_zoom - 0.1).max(0.7);
                    ui.ctx().set_zoom_factor(self.config.ui_zoom);
                    let _ = self.config.save();
                } else if i.key_pressed(egui::Key::Num0) {
                    self.config.ui_zoom = 1.20;
                    ui.ctx().set_zoom_factor(self.config.ui_zoom);
                    let _ = self.config.save();
                }
            }
        });

        self.subtitle_state.poll(&self.log_sender);
        self.transcription_state.poll(&self.log_sender);
        self.content_creator_state.poll(&self.log_sender);

        let mut received_new_logs = false;
        while let Ok(msg) = self.log_receiver.try_recv() {
            let stamped_msg = format!("[{}] {}", current_log_timestamp(), msg);
            self.logs.push(stamped_msg);
            received_new_logs = true;
        }

        if received_new_logs {
            ui.ctx().request_repaint();
        }

        let is_busy = self.subtitle_state.job.is_processing
            || self.transcription_state.job.is_processing
            || self.content_creator_state.job.is_processing;

        if is_busy {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }

        egui::Panel::left("tabs_panel").show(ui, |ui| {
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Image::new(egui::include_image!("../assets/logo.png"))
                            .fit_to_exact_size(egui::vec2(120.0, 120.0)),
                    );
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::SubtitleCreator,
                    "Subtitle Creator",
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::AudioTranscription,
                    "Audio Transcription",
                );
                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::ContentCreator,
                    "YouTube & Patreon",
                );

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(4.0);

                ui.selectable_value(
                    &mut self.current_tab,
                    Tab::SetUpLocalModel,
                    "Set up local model",
                );
                ui.selectable_value(&mut self.current_tab, Tab::Configuration, "Configuration");
                ui.selectable_value(&mut self.current_tab, Tab::About, "About");
                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                let (icon, label) = if self.config.dark_mode {
                    ("🌙", "Dark Mode")
                } else {
                    ("☀️", "Light Mode")
                };

                if ui.button(format!("{} {}", icon, label)).clicked() {
                    self.config.dark_mode = !self.config.dark_mode;

                    // Switch egui visuals dynamically
                    let visuals = if self.config.dark_mode {
                        egui::Visuals::dark()
                    } else {
                        egui::Visuals::light()
                    };
                    ui.ctx().set_visuals(visuals);

                    // Save to config.toml
                    let _ = self.config.save();
                }
            });
        });

        let total_window_height = ui.max_rect().height();
        let target_logs_height = total_window_height / 3.0;

        egui::Panel::bottom("logs_bottom_v3")
            .resizable(true)
            .default_size(target_logs_height)
            .min_size(100.0)
            .max_size(total_window_height * 0.40)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("📋 Logs ({})", self.logs.len())).strong(),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("🔄 Check Services").clicked() {
                            podman::check_services(&self.config, self.log_sender.clone(), 0);
                        }
                        if ui.button("🗑 Clear Logs").clicked() {
                            self.logs.clear();
                        }
                    });
                });
                ui.separator();

                egui::Frame::canvas(ui.style())
                    .fill(egui::Color32::from_rgb(20, 22, 25))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 60, 60)))
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        let text_style = egui::TextStyle::Monospace;
                        let row_height = ui.text_style_height(&text_style);
                        let total_rows = self.logs.len();

                        // Virtualized scrolling avoids allocating widgets for all offscreen rows
                        egui::ScrollArea::vertical()
                            .stick_to_bottom(true)
                            .auto_shrink([false, true])
                            .show_rows(ui, row_height, total_rows, |ui, row_range| {
                                ui.set_width(ui.available_width());
                                for idx in row_range {
                                    if let Some(log) = self.logs.get(idx) {
                                        ui.label(
                                            egui::RichText::new(log)
                                                .font(egui::FontId::monospace(11.5))
                                                .color(egui::Color32::from_rgb(215, 215, 215)),
                                        );
                                    }
                                }
                            });
                    });
            });

        egui::CentralPanel::default().show(ui, |ui| match self.current_tab {
            Tab::SubtitleCreator => {
                subtitle_creator::show(&mut self.subtitle_state, &self.config, &self.log_sender, ui)
            }
            Tab::AudioTranscription => transcription::show(
                &mut self.transcription_state,
                &self.config,
                &self.log_sender,
                ui,
            ),
            Tab::ContentCreator => content_creator::show(
                &mut self.content_creator_state,
                &self.config,
                &self.log_sender,
                ui,
            ),
            Tab::SetUpLocalModel => setup_model::show(ui),
            Tab::Configuration => config::show(&mut self.config, &self.log_sender, ui),
            Tab::About => about::show(ui),
        });
    }
}
