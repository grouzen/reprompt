use std::time::Instant;

use egui::{CornerRadius, Frame, ScrollArea, Stroke};
use egui_modal::Modal;

pub const TITLE: &str = "Reprompt";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RepromptApp {
    prompts: Vec<Prompt>,
    #[serde(skip)]
    new_prompt_title: String,
    #[serde(skip)]
    new_prompt_content: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Prompt {
    title: String,
    content: String,
    history: Vec<PromptResponse>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PromptResponse {
    input: String,
    output: String,
    #[serde(skip)]
    requested_at: Instant,
}

impl Default for RepromptApp {
    fn default() -> Self {
        Self {
            prompts: Vec::new(),
            new_prompt_title: String::with_capacity(256),
            new_prompt_content: String::with_capacity(1024),
        }
    }
}

impl Default for Prompt {
    fn default() -> Self {
        Self {
            title: Default::default(),
            content: Default::default(),
            history: Default::default(),
        }
    }
}

impl Default for PromptResponse {
    fn default() -> Self {
        Self {
            input: Default::default(),
            output: Default::default(),
            requested_at: Instant::now(),
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
        egui::SidePanel::left("prompts")
            .resizable(true)
            .show(ctx, |ui| {
                self.show_prompts(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Reprompt!");
        });
    }

    fn show_prompts(&mut self, ui: &mut egui::Ui) {
        let add_prompt_modal = Modal::new(ui.ctx(), "add_prompt_modal");

        ui.horizontal_top(|ui| {
            if ui.button("➕").clicked() {
                add_prompt_modal.open();
            }
        });

        ScrollArea::vertical().show(ui, |ui| {
            for prompt in self.prompts.iter() {
                prompt.show(ui);
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

            let prompt = Prompt {
                title: self.new_prompt_title.clone(),
                content: self.new_prompt_content.clone(),
                history: Vec::new(),
            };

            self.new_prompt_title.clear();
            self.new_prompt_content.clear();

            self.prompts.push(prompt);
        };

        if modal.button(ui, "Cancel").clicked() {
            modal.close();
        }
    }
}

impl Prompt {
    pub fn show(&self, ui: &mut egui::Ui) {
        Frame::group(ui.style())
            .corner_radius(CornerRadius::same(6))
            .stroke(Stroke::new(2.0, ui.style().visuals.window_stroke.color))
            .show(ui, |ui| ui.label(self.title.clone()));
    }
}
