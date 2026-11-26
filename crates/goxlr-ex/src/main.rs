#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod backend;
mod runtime;

fn main() {
    #[cfg(windows)]
    let Some(_instance) = runtime::Instance::acquire() else {
        return;
    };
    let runtime = runtime::Runtime::start();
    let boot_error = runtime.as_ref().err().cloned().unwrap_or_default();
    let data = runtime
        .as_ref()
        .map(|r| r.data.clone())
        .unwrap_or_else(|_| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .join("GoXLR-EX-Rust-data")
        });
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1500.0, 780.0])
            .with_min_inner_size([1000.0, 650.0])
            .with_title("GoXLR EX — Rust")
            .with_icon({
                let logo = image::load_from_memory(include_bytes!("../assets/logo.png"))
                    .unwrap()
                    .thumbnail(116, 116)
                    .into_rgba8();
                let mut icon = image::RgbaImage::new(128, 128);
                image::imageops::overlay(
                    &mut icon,
                    &logo,
                    (128 - logo.width()) as i64 / 2,
                    (128 - logo.height()) as i64 / 2,
                );
                eframe::egui::IconData {
                    rgba: icon.into_raw(),
                    width: 128,
                    height: 128,
                }
            }),
        renderer: eframe::Renderer::Glow,
        centered: true,
        ..Default::default()
    };
    let result = eframe::run_native(
        "GoXLR EX — Rust",
        options,
        Box::new(move |cc| Ok(Box::new(app::Studio::new(cc, data, boot_error)))),
    );
    if let Err(error) = result {
        let _ = std::fs::write(
            std::env::current_exe().unwrap().with_extension("error.txt"),
            error.to_string(),
        );
    }
    drop(runtime);
}
