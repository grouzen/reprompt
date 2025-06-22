use egui::{Layout, ScrollArea};
use egui_modal::{Modal, ModalStyle};
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
    view_state: ViewState,
    #[serde(skip)]
    tokio_runtime: runtime::Runtime,
    #[serde(skip)]
    ollama: Ollama,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
struct ViewState {
    modal: ViewModalState,
    main_panel: ViewMainPanelState,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
enum ViewModalState {
    #[default]
    None,
    AddPrompt,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
enum ViewMainPanelState {
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
            view_state: Default::default(),
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
        self.show_left_panel(ctx);
        self.show_main_panel(ctx);
    }

    fn show_left_panel(&mut self, ctx: &egui::Context) {
        let available_width = ctx.available_rect().width();
        let max_width = available_width * 0.3;
        let min_width = available_width * 0.2;

        egui::SidePanel::left("left_panel_prompts")
            .resizable(true)
            .max_width(max_width)
            .min_width(min_width)
            .show(ctx, |ui| {
                let add_prompt_modal_width = available_width * 0.5;
                let add_prompt_modal = self.create_add_prompt_modal(ui, add_prompt_modal_width);

                ui.horizontal_top(|ui| {
                    if ui.button("➕").clicked() {
                        add_prompt_modal.open();
                        self.view_state.modal = ViewModalState::AddPrompt;
                    }
                });

                self.show_left_panel_prompts(ui);

                if add_prompt_modal.was_outside_clicked() {
                    self.view_state.modal = ViewModalState::None;
                }

                add_prompt_modal.show(|ui| {
                    self.show_add_prompt_modal(ui, &add_prompt_modal, add_prompt_modal_width);
                });
            });
    }

    fn show_left_panel_prompts(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical().show(ui, |ui| {
            for (idx, prompt) in self.prompts.iter().enumerate() {
                let selected = self.is_prompt_selected(idx);

                ui.add_space(3.0);
                prompt.show_left_panel(ui, selected, || {
                    self.view_state.main_panel = ViewMainPanelState::Prompt(idx)
                });
            }
        });
    }

    fn show_main_panel(&mut self, ctx: &egui::Context) {
        let covered = self.is_covered();

        egui::CentralPanel::default().show(ctx, |ui| match self.view_state.main_panel {
            ViewMainPanelState::MixedHistory => {
                ui.label("Reprompt!");
            }
            ViewMainPanelState::Prompt(idx) => {
                if let Some(prompt) = self.prompts.get_mut(idx) {
                    prompt.show_main_panel(ui, covered, &self.tokio_runtime, &self.ollama);

                    if prompt.is_generating() {
                        ctx.request_repaint();
                    }
                }
            }
        });
    }

    fn create_add_prompt_modal(&mut self, ui: &mut egui::Ui, width: f32) -> Modal {
        let style = ModalStyle {
            default_width: Some(width),
            ..Default::default()
        };

        Modal::new(ui.ctx(), "add_prompt_modal")
            .with_close_on_outside_click(true)
            .with_style(&style)
    }

    fn show_add_prompt_modal(&mut self, ui: &mut egui::Ui, modal: &Modal, width: f32) {
        modal.frame(ui, |ui| {
            egui::TextEdit::singleline(&mut self.new_prompt_title)
                .hint_text("Write the title of your prompt here")
                .desired_width(width)
                .show(ui);

            egui::TextEdit::multiline(&mut self.new_prompt_content)
                .desired_rows(10)
                .hint_text(
                    "Write the content of your prompt here. It will be prepended to all requests.",
                )
                .desired_width(width)
                .show(ui);
        });

        ui.with_layout(
            Layout::right_to_left(egui::Align::TOP).with_main_align(egui::Align::RIGHT),
            |ui| {
                if modal.caution_button(ui, "Cancel").clicked() {
                    self.view_state.modal = ViewModalState::None;
                    self.new_prompt_title.clear();
                    self.new_prompt_content.clear();
                };

                if modal.button(ui, "Create").clicked() {
                    let id = self.prompts.len();
                    let prompt = Prompt::new(
                        self.new_prompt_title.clone(),
                        self.new_prompt_content.clone(),
                        id,
                    );

                    self.new_prompt_title.clear();
                    self.new_prompt_content.clear();

                    self.prompts.push(prompt);

                    modal.close();
                    self.view_state.modal = ViewModalState::None;
                };
            },
        );
    }

    fn is_covered(&self) -> bool {
        !matches!(self.view_state.modal, ViewModalState::None)
    }

    fn is_prompt_selected(&self, idx: usize) -> bool {
        matches!(self.view_state.main_panel, ViewMainPanelState::Prompt(idx0) if idx0 == idx)
    }
}
