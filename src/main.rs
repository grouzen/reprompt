use reprompt::app::App;

fn main() -> eframe::Result {
    env_logger::init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default(),
        ..Default::default()
    };

    eframe::run_native(
        reprompt::app::TITLE,
        native_options,
        Box::new(|cc| {
            let app = App::from_eframe_context(cc);

            app.load_local_models();

            Ok(Box::new(app))
        }),
    )
}
