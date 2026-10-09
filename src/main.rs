mod app;
mod canvas;
mod category;
mod history;
mod persist;
mod reference;
mod stroke;
mod submit;
mod timer;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("artproj"),
        ..Default::default()
    };
    eframe::run_native(
        "artproj",
        options,
        Box::new(|_cc| Ok(Box::new(app::SketchApp::new()))),
    )
}
