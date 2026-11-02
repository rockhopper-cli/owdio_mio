mod app;
mod audio;
mod clients;
mod config;
mod container;
mod pipelines;
mod subtitles;
mod ui;
mod utils;

use eframe::egui;

fn main() -> eframe::Result {
    // Force native Wayland backend if in a Wayland session
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        unsafe {
            std::env::set_var("WINIT_UNIX_BACKEND", "wayland");
        }
    }

    let loaded_config = config::AppConfig::load();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_app_id("owdio_mio")
            .with_maximized(true)
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Owdio Mio",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);

            // Vector UI zoom (defaults to 1.20): scales checkboxes, text, and layout proportionally
            cc.egui_ctx.set_zoom_factor(loaded_config.ui_zoom);

            load_custom_font(&cc.egui_ctx);

            if loaded_config.dark_mode {
                cc.egui_ctx.set_visuals(egui::Visuals::dark());
            } else {
                cc.egui_ctx.set_visuals(egui::Visuals::light());
            }

            Ok(Box::new(app::OwdioMio::new(loaded_config)))
        }),
    )
}

fn load_custom_font(ctx: &eframe::egui::Context) {
    let mut fonts = eframe::egui::FontDefinitions::default();
    fonts.font_data.insert(
        "my_system_font".to_owned(),
        std::sync::Arc::new(eframe::egui::FontData::from_static(include_bytes!(
            "../assets/InclusiveSans-VariableFont_wght.ttf"
        ))),
    );
    fonts
        .families
        .entry(eframe::egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "my_system_font".to_owned());
    ctx.set_fonts(fonts);
}
