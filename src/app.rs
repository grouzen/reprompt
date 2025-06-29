use egui::{Button, Color32, Layout, ScrollArea, Stroke, WidgetText};
use egui_commonmark::CommonMarkCache;
use egui_modal::{Icon, Modal, ModalStyle};
use flowync::{CompactFlower, error::Compact};
use ollama_rs::{Ollama, models::LocalModel};
use tokio::runtime;

use crate::{ollama::OllamaClient, prompt::Prompt};

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
    ollama_client: OllamaClient,
    ollama_models: OllamaModels,
    #[serde(skip)]
    commonmark_cache: CommonMarkCache,
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
    RemovePrompt(usize),
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
enum ViewMainPanelState {
    #[default]
    MixedHistory,
    Prompt(usize),
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct OllamaModels {
    selected: Option<LocalModel>,
    #[serde(skip)]
    available: Vec<LocalModel>,
    #[serde(skip)]
    load_flower: LoadLocalModelsFlower,
}

type LoadLocalModelsFlower =
    CompactFlower<Vec<LocalModel>, (Vec<LocalModel>, Option<LocalModel>), anyhow::Error>;

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
            ollama_client: OllamaClient::new(Ollama::default()),
            ollama_models: Default::default(),
            commonmark_cache: CommonMarkCache::default(),
        }
    }
}

impl Default for OllamaModels {
    fn default() -> Self {
        Self {
            selected: Default::default(),
            available: Default::default(),
            load_flower: LoadLocalModelsFlower::new(1),
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
        ctx.set_zoom_factor(1.2);
    }

    fn show(&mut self, ctx: &egui::Context) {
        self.show_left_panel(ctx);
        self.show_main_panel(ctx);

        if self.ollama_models.load_flower.is_active() {
            self.poll_load_flower();
        }
    }

    pub fn load_local_models(&self) {
        let handle = self.ollama_models.load_flower.handle();
        let ollama_client = self.ollama_client.clone();
        let current_selected = self.ollama_models.selected.clone();

        self.tokio_runtime.spawn(async move {
            handle.activate();

            match ollama_client.list_models().await {
                Ok(response) => {
                    let mut maybe_selected = None;

                    if let Some(default) = response.first() {
                        match current_selected {
                            Some(selected) => {
                                if !response
                                    .iter()
                                    .map(|m| &m.name)
                                    .cloned()
                                    .collect::<Vec<String>>()
                                    .contains(&selected.name)
                                {
                                    maybe_selected = Some(default.clone());
                                }
                            }
                            None => maybe_selected = Some(default.clone()),
                        }
                    }

                    handle.success((response, maybe_selected))
                }
                Err(e) => handle.error(e),
            }
        });
    }

    fn poll_load_flower(&mut self) {
        self.ollama_models
            .load_flower
            .extract(|models| {
                self.ollama_models.available = models;
            })
            .finalize(|result| match result {
                Ok((models, maybe_selected)) => {
                    self.ollama_models.available = models;
                    if let Some(selected) = maybe_selected {
                        self.ollama_models.selected = Some(selected);
                    }
                }
                Err(Compact::Suppose(_)) => (),
                Err(Compact::Panicked(_)) => (),
            });
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

                ui.add_space(6.0);

                ui.horizontal_top(|ui| {
                    if ui
                        .add(
                            egui::Button::new("➕")
                                .fill(Color32::TRANSPARENT)
                                .small()
                                .stroke(Stroke::NONE),
                        )
                        .on_hover_text("Create new prompt")
                        .clicked()
                    {
                        add_prompt_modal.open();
                        self.view_state.modal = ViewModalState::AddPrompt;
                    }

                    if let Some(selected) = &self.ollama_models.selected {
                        let mut clicked = None;

                        egui::ComboBox::from_id_salt("left_panel_models_selector")
                            .selected_text(&selected.name)
                            .show_ui(ui, |ui| {
                                for model in &self.ollama_models.available {
                                    let checked = selected.name == model.name;
                                    if ui.selectable_label(checked, &model.name).clicked() {
                                        clicked = Some(model.clone());
                                    }
                                }
                            });

                        if let Some(clicked) = clicked {
                            self.ollama_models.selected = Some(clicked);
                        }
                    }
                });

                ui.separator();

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
            let remove_prompt_modal =
                Modal::new(ui.ctx(), "remove_prompt_modal").with_close_on_outside_click(true);

            for (idx, prompt) in self.prompts.iter().enumerate() {
                let selected = self.is_prompt_selected(idx);

                ui.add_space(3.0);

                prompt.show_left_panel(
                    ui,
                    selected,
                    || self.view_state.main_panel = ViewMainPanelState::Prompt(idx),
                    || {
                        remove_prompt_modal.open();
                        self.view_state.modal = ViewModalState::RemovePrompt(idx);
                    },
                );

                if remove_prompt_modal.was_outside_clicked() {
                    self.view_state.modal = ViewModalState::None;
                }
            }

            remove_prompt_modal.show(|ui| {
                self.show_remove_prompt_modal(
                    ui,
                    &remove_prompt_modal,
                    |s| s.view_state.modal = ViewModalState::None,
                    |s, idx| {
                        s.view_state.modal = ViewModalState::None;
                        s.prompts.remove(idx);
                    },
                );
            });
        });
    }

    fn show_remove_prompt_modal<F, G>(
        &mut self,
        ui: &mut egui::Ui,
        modal: &Modal,
        on_cancel: F,
        on_remove: G,
    ) where
        F: Fn(&mut Self),
        G: FnOnce(&mut Self, usize),
    {
        modal.title(ui, "Remove Prompt");
        modal.body_and_icon(
            ui,
            "Do you really want to remove this prompt?",
            Icon::Warning,
        );

        modal.buttons(ui, |ui| {
            if modal.button(ui, "Cancel").clicked() {
                on_cancel(self);
            }

            if modal.caution_button(ui, "Remove").clicked() {
                match self.view_state.modal {
                    ViewModalState::RemovePrompt(idx) => {
                        on_remove(self, idx);
                    }
                    _ => on_cancel(self),
                }
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
                    prompt.show_main_panel(
                        ui,
                        covered,
                        &self.tokio_runtime,
                        &self.ollama_client,
                        &mut self.commonmark_cache,
                    );

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
                // TODO: the buttons in egui_modal do not allow overriding the default behavior when they close the modal upon clicking
                let cancel_button =
                    Button::new(WidgetText::from("Cancel").color(Color32::from_rgb(242, 148, 148)))
                        .fill(Color32::from_rgb(87, 38, 34));
                let create_button =
                    Button::new(WidgetText::from("Create").color(Color32::from_rgb(141, 182, 242)))
                        .fill(Color32::from_rgb(33, 54, 84));

                if ui.add(cancel_button).clicked() {
                    self.new_prompt_title.clear();
                    self.new_prompt_content.clear();

                    modal.close();
                    self.view_state.modal = ViewModalState::None;
                }

                if ui.add(create_button).clicked()
                    && !self.new_prompt_title.is_empty()
                    && !self.new_prompt_content.is_empty()
                {
                    let id = self.prompts.len();
                    let prompt = Prompt::new(
                        self.new_prompt_title.clone(),
                        self.new_prompt_content.clone(),
                        id,
                    );
                    self.prompts.push(prompt);

                    self.new_prompt_title.clear();
                    self.new_prompt_content.clear();

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
