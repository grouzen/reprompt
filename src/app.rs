use egui::ScrollArea;
use egui_modal::Modal;
use ollama_rs::Ollama;
use tokio::runtime;

use crate::prompt::Prompt;

pub const TITLE: &str = "Reprompt";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RepromptApp {
    prompts: Vec<Prompt>,
    #[serde(skip)]
    new_prompt_title: String,
    #[serde(skip)]
    new_prompt_content: String,
    main_view: MainView,
    #[serde(skip)]
    tokio_runtime: runtime::Runtime,
    #[serde(skip)]
    ollama: Ollama,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
enum MainView {
    #[default]
    MixedHistory,
    Prompt(usize),
}

impl Default for RepromptApp {
    fn default() -> Self {
        Self {
            prompts: Vec::new(),
            new_prompt_title: String::with_capacity(256),
            new_prompt_content: String::with_capacity(1024),
            main_view: MainView::MixedHistory,
            tokio_runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
            ollama: Ollama::default(),
        }
    }
}

impl eframe::App for RepromptApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.show(ctx)
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}

impl RepromptApp {
    pub fn from_eframe_context(cc: &eframe::CreationContext<'_>) -> Self {
        eframe::storage_dir(TITLE);

        Self::set_style(&cc.egui_ctx);

        match cc.storage {
            Some(storage) => eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default(),
            None => Default::default(),
        }
    }

    fn set_style(ctx: &egui::Context) {
        ctx.set_zoom_factor(1.5);
    }

    fn show(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("left_panel_prompts")
            .resizable(true)
            .show(ctx, |ui| {
                self.show_left_panel_prompts(ui);
                ui.allocate_space(ui.available_size());
            });

        egui::CentralPanel::default().show(ctx, |ui| match self.main_view {
            MainView::MixedHistory => {
                ui.label("Reprompt!");
            }
            MainView::Prompt(idx) => {
                if let Some(prompt) = self.prompts.get_mut(idx) {
                    prompt.show_main_panel(ui, &self.tokio_runtime, &self.ollama);
                }
            }
        });
    }

    fn show_left_panel_prompts(&mut self, ui: &mut egui::Ui) {
        let add_prompt_modal = Modal::new(ui.ctx(), "add_prompt_modal");

        ui.horizontal_top(|ui| {
            if ui.button("➕").clicked() {
                add_prompt_modal.open();
            }
        });

        ScrollArea::vertical().show(ui, |ui| {
            for (idx, prompt) in self.prompts.iter().enumerate() {
                prompt.show_left_panel(ui, || self.main_view = MainView::Prompt(idx));
            }
        });

        add_prompt_modal.show(|ui| {
            self.show_add_prompt_modal(ui, &add_prompt_modal);
        });
    }

    fn show_add_prompt_modal(&mut self, ui: &mut egui::Ui, modal: &Modal) {
        modal.frame(ui, |ui| {
            ui.text_edit_singleline(&mut self.new_prompt_title);
            ui.text_edit_multiline(&mut self.new_prompt_content);
        });

        if modal.button(ui, "Create").clicked() {
            modal.close();

            let prompt = Prompt::new(
                self.new_prompt_title.clone(),
                self.new_prompt_content.clone(),
            );

            self.new_prompt_title.clear();
            self.new_prompt_content.clear();

            self.prompts.push(prompt);
        };

        if modal.button(ui, "Cancel").clicked() {
            modal.close();
        }
    }
}
