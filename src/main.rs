use reprompt::app::RepromptApp;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default(),
        ..Default::default()
    };

    eframe::run_native(
        "Reprompt",
        native_options,
        Box::new(|_| Ok(Box::new(RepromptApp::default()))),
    )
}
