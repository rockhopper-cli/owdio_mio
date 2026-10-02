// src/ui/components.rs
use eframe::egui;
use std::path::Path;

/// Renders a unified file picker button and selected path label.
/// Expands support beyond `.wav` to all formats that FFmpeg can process.
pub fn audio_file_picker(
    ui: &mut egui::Ui,
    file_path: &mut Option<String>,
    default_dir: &Path,
    is_processing: bool,
) {
    ui.horizontal(|ui| {
        let pick_btn = ui.add_enabled(
            !is_processing,
            egui::Button::new("Select Audio/Video File"),
        );

        if pick_btn.clicked() {
            let mut dialog = rfd::FileDialog::new().add_filter(
                "Audio/Video Files",
                &["wav", "mp3", "m4a", "flac", "ogg", "opus", "mp4", "mkv", "aac", "wma"],
            );

            if default_dir.exists() {
                dialog = dialog.set_directory(default_dir);
            }

            if let Some(path) = dialog.pick_file() {
                *file_path = Some(path.display().to_string());
            }
        }

        if let Some(path) = file_path {
            ui.label(path.as_str());
        } else {
            ui.label(egui::RichText::new("No file selected.").color(egui::Color32::RED));
        }
    });
}

/// Renders a standardized animated progress bar.
pub fn render_progress_bar(
    ui: &mut egui::Ui,
    progress: f32,
    progress_text: &str,
    is_processing: bool,
) {
    ui.horizontal(|ui| {
        let progress_bar = egui::ProgressBar::new(progress)
            .text(progress_text)
            .animate(is_processing);
        ui.add(progress_bar);
    });
}