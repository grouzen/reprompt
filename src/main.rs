use reprompt::app::RepromptApp;

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default(),
        ..Default::default()
    };

    eframe::run_native(
        reprompt::app::TITLE,
        native_options,
        Box::new(|cc| Ok(Box::new(RepromptApp::new(cc)))),
    )
}
