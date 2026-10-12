// src/app.rs
use crate::config_struct::AppConfig;
use crate::podman::manager;
use crate::ui::{about, config, content_creator, setup_model, subtitle_creator, transcription};
use crate::utils::current_log_timestamp;
use eframe::egui;
use std::sync::mpsc::{Receiver, Sender, channel};
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

impl Default for OwdioMio {
    fn default() -> Self {
        let loaded_config = AppConfig::load();
        let (log_sender, log_receiver) = channel();

        // 1. Startup: Launch enabled containers concurrently
        if loaded_config.manage_whisper_container {
            manager::start_container(
                &loaded_config.whisper_container_name,
                Some(log_sender.clone()),
            );
        }
        if loaded_config.manage_qwen_container {
            manager::start_container(&loaded_config.qwen_container_name, Some(log_sender.clone()));
        }

        // 2. Delayed initial health check so containers have time to spin up
        manager::check_services(&loaded_config, log_sender.clone(), 2500);

        Self {
            current_tab: Tab::SubtitleCreator,
            subtitle_state: subtitle_creator::SubtitleState::new(&loaded_config),
            transcription_state: transcription::TranscriptionState::default(),
            content_creator_state: content_creator::ContentCreatorState::default(),
            config: loaded_config,
            logs: vec!["[Owdio Mio] Application starting up...".to_string()],
            log_sender,
            log_receiver,
        }
    }
}

impl Drop for OwdioMio {
    fn drop(&mut self) {
        if self.config.manage_whisper_container {
            manager::stop_container(&self.config.whisper_container_name, None);
        }
        if self.config.manage_qwen_container {
            manager::stop_container(&self.config.qwen_container_name, None);
        }
    }
}

impl eframe::App for OwdioMio {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.subtitle_state.poll(&self.log_sender);
        self.transcription_state.poll(&self.log_sender);
        self.content_creator_state.poll(&self.log_sender);

        while let Ok(msg) = self.log_receiver.try_recv() {
            let stamped_msg = format!("[{}] {}", current_log_timestamp(), msg);
            self.logs.push(stamped_msg);
        }

        // Throttle UI repainting during processing to ~20 FPS (prevents 100% CPU usage)
        let is_busy = self.subtitle_state.is_processing
            || self.transcription_state.is_processing
            || self.content_creator_state.is_processing;

        if is_busy {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }

        // Navigation Panel
        egui::Panel::left("tabs_panel").show(ui, |ui| {
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Image::new(egui::include_image!("../assets/logo.png"))
                            .fit_to_exact_size(egui::vec2(128.0, 128.0)),
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
            });
        });

        // Bottom Logs Panel
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
                            manager::check_services(&self.config, self.log_sender.clone(), 0);
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
                        egui::ScrollArea::vertical()
                            .stick_to_bottom(true)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                for log in &self.logs {
                                    ui.label(
                                        egui::RichText::new(log)
                                            .font(egui::FontId::monospace(11.5))
                                            .color(egui::Color32::from_rgb(215, 215, 215)),
                                    );
                                }
                            });
                    });
            });

        // Central Panel
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
