// src/main.rs
mod app;
mod audio;
mod audio_pipeline;
mod podman;
mod subtitle_pipeline;
mod subtitles;
mod ui;
mod utils;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_maximized(true) // Opens maximized in full screen
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Owdio Mio",
        options,
        Box::new(|cc| {
            // Install image loaders (decodes PNG, JPG, SVG, etc.)
            egui_extras::install_image_loaders(&cc.egui_ctx);

            // Scale everything up by 30%
            cc.egui_ctx.set_zoom_factor(1.3);

            // Load custom Inclusive Sans font
            load_custom_font(&cc.egui_ctx);

            Ok(Box::new(app::OwdioMio::default()))
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
