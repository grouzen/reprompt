pub struct RepromptApp {}

impl Default for RepromptApp {
    fn default() -> Self {
        Self {}
    }
}

impl eframe::App for RepromptApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Reprompt!");
        });
    }
}
