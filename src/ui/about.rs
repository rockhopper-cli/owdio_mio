// src/ui/about.rs
use eframe::egui;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_NAME: &str = env!("CARGO_PKG_NAME");

pub fn show(ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .id_salt("about_scroll_area")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.add_space(8.0);

            // 1. Header with logo and dynamic version
            ui.horizontal(|ui| {
                ui.add(
                    egui::Image::new(egui::include_image!("../../assets/logo.png"))
                        .fit_to_exact_size(egui::vec2(84.0, 84.0)),
                );
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.heading(
                        egui::RichText::new("Owdio Mio")
                            .color(egui::Color32::from_rgb(255, 180, 50))
                            .strong()
                            .size(28.0),
                    );
                    ui.label(
                        egui::RichText::new(format!("Version {}", APP_VERSION))
                            .monospace()
                            .color(egui::Color32::LIGHT_GREEN),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Privacy-First, 100% Offline Audio Transcriber & Subtitle Generator",
                        )
                        .italics(),
                    );
                });
            });

            ui.add_space(14.0);
            ui.separator();
            ui.add_space(10.0);

            // 2. Overview
            ui.label(egui::RichText::new("About Owdio Mio").strong().size(18.0));
            ui.add_space(4.0);
            ui.label(
                "Owdio Mio is a native Linux desktop application built with Rust and egui. \
                It provides local speech-to-text transcription, collision-free .srt subtitle \
                generation, and AI-powered copy creation for YouTube and Patreon.",
            );

            ui.add_space(12.0);

            // 3. Technical Stack & Privacy Highlights
            ui.label(egui::RichText::new("Architecture & Privacy").strong().size(18.0));
            ui.add_space(4.0);

            egui::Grid::new("about_info_grid")
                .num_columns(2)
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("🎞 Audio Extraction:").strong());
                    ui.label("FFmpeg for resampling (16kHz Mono PCM) and audio segmentation.");
                    ui.end_row();

                    ui.label(egui::RichText::new("🎨 GUI Framework:").strong());
                    ui.label("egui & eframe (Rust 2024 edition).");
                    ui.end_row();

                    ui.label(egui::RichText::new("📜 License:").strong());
                    ui.label("MIT License");
                    ui.end_row();
                });

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(10.0);

            // 4. Build Metadata Box
            ui.label(egui::RichText::new("Build Information").strong().size(16.0));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "The version number is compiled directly from Cargo.toml via the CARGO_PKG_VERSION macro.",
                )
                .weak()
                .small(),
            );

            ui.add_space(8.0);
            egui::Frame::group(ui.style())
                .fill(ui.visuals().code_bg_color)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.monospace(format!("Package Name   : {}", APP_NAME));
                    ui.monospace(format!("Package Version: {}", APP_VERSION));
                    ui.monospace("Rust Edition   : 2024");
                });

            ui.add_space(16.0);
        });
}
