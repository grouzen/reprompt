use egui::{Button, Color32, Layout, ScrollArea, Stroke, WidgetText};
use egui_commonmark::CommonMarkCache;
use egui_modal::{Icon, Modal, ModalStyle};
use egui_theme_switch::global_theme_switch;
use flowync::{CompactFlower, error::Compact};
use ollama_rs::{Ollama, models::LocalModel};
use tokio::runtime;

use crate::{assign_if_some, ollama::OllamaClient, prompt::Prompt};

pub const TITLE: &str = "Reprompt";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RepromptApp {
    prompts: Vec<Prompt>,
    view_state: ViewState,
    ollama_models: OllamaModels,
    #[serde(skip)]
    new_prompt_title: String,
    #[serde(skip)]
    new_prompt_content: String,
    #[serde(skip)]
    tokio_runtime: runtime::Runtime,
    #[serde(skip)]
    ollama_client: OllamaClient,
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

#[derive(Debug)]
pub enum Action {
    OpenAddPromptDialog,
    CloseAddPromptDialog,
    CancelPromptCreation,
    CreatePrompt,
    OpenRemovePromptDialog(usize),
    CloseRemovePromptDialog,
    RemovePrompt(usize),
    SelectPrompt(usize),
    SelectOllamaModel(LocalModel),
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
        let add_prompt_modal =
            Self::create_add_prompt_modal(ctx, ctx.available_rect().width() * 0.5);
        let remove_prompt_modal =
            Modal::new(ctx, "remove_prompt_modal").with_close_on_outside_click(true);

        let action = self.show(ctx, &add_prompt_modal, &remove_prompt_modal);

        self.handle_action(action, &add_prompt_modal, &remove_prompt_modal);
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

    fn handle_action(
        &mut self,
        action: Option<Action>,
        add_prompt_modal: &Modal,
        remove_prompt_modal: &Modal,
    ) {
        if let Some(action) = action {
            match action {
                Action::OpenAddPromptDialog => {
                    add_prompt_modal.open();
                    self.view_state.modal = ViewModalState::AddPrompt;
                }
                Action::CloseAddPromptDialog => {
                    self.view_state.modal = ViewModalState::None;
                }
                Action::CancelPromptCreation => {
                    self.new_prompt_title.clear();
                    self.new_prompt_content.clear();

                    add_prompt_modal.close();
                    self.view_state.modal = ViewModalState::None;
                }
                Action::CreatePrompt => {
                    let id = self.prompts.len();
                    let prompt = Prompt::new(
                        self.new_prompt_title.clone(),
                        self.new_prompt_content.clone(),
                        id,
                    );
                    self.prompts.push(prompt);

                    self.new_prompt_title.clear();
                    self.new_prompt_content.clear();

                    add_prompt_modal.close();
                    self.view_state.modal = ViewModalState::None;
                }
                Action::OpenRemovePromptDialog(idx) => {
                    remove_prompt_modal.open();
                    self.view_state.modal = ViewModalState::RemovePrompt(idx);
                }
                Action::CloseRemovePromptDialog => {
                    self.view_state.modal = ViewModalState::None;
                }
                Action::RemovePrompt(idx) => {
                    self.view_state.modal = ViewModalState::None;
                    self.prompts.remove(idx);
                }
                Action::SelectPrompt(idx) => {
                    self.view_state.main_panel = ViewMainPanelState::Prompt(idx);
                }
                Action::SelectOllamaModel(local_model) => {
                    self.ollama_models.selected = Some(local_model);
                }
            }
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
                    let maybe_selected =
                        response.first().and_then(|default| match current_selected {
                            Some(selected)
                                if !response
                                    .iter()
                                    .map(|m| &m.name)
                                    .cloned()
                                    .collect::<Vec<String>>()
                                    .contains(&selected.name) =>
                            {
                                Some(default.clone())
                            }
                            None => Some(default.clone()),
                            _ => None,
                        });

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

    fn show(
        &mut self,
        ctx: &egui::Context,
        add_prompt_modal: &Modal,
        remove_prompt_modal: &Modal,
    ) -> Option<Action> {
        let action = self.show_left_panel(ctx, add_prompt_modal, remove_prompt_modal);

        self.show_main_panel(ctx);

        if self.ollama_models.load_flower.is_active() {
            self.poll_load_flower();
        }

        action
    }

    fn show_left_panel(
        &mut self,
        ctx: &egui::Context,
        add_prompt_modal: &Modal,
        remove_prompt_modal: &Modal,
    ) -> Option<Action> {
        let (max_width, min_width) = Self::get_left_panel_width(ctx);
        let mut action = None;

        egui::SidePanel::left("left_panel_prompts")
            .resizable(true)
            .max_width(max_width)
            .min_width(min_width)
            .show(ctx, |ui| {
                ui.add_space(6.0);

                ui.horizontal_top(|ui| {
                    assign_if_some!(action, self.show_left_panel_create_protmp_button(ui));

                    assign_if_some!(action, self.show_left_panel_model_selector(ui))
                });

                ui.separator();

                assign_if_some!(
                    action,
                    self.show_left_panel_prompts(ui, remove_prompt_modal)
                );

                ui.with_layout(Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(6.0);
                    global_theme_switch(ui);
                });

                if add_prompt_modal.was_outside_clicked() {
                    action = Some(Action::CloseAddPromptDialog);
                }

                add_prompt_modal.show(|ui| {
                    assign_if_some!(action, self.show_add_prompt_modal(ui, add_prompt_modal));
                });
            });

        action
    }

    fn show_left_panel_create_protmp_button(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;

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
            action = Some(Action::OpenAddPromptDialog);
        }

        action
    }

    fn show_left_panel_model_selector(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;

        if let Some(selected) = &self.ollama_models.selected {
            egui::ComboBox::from_id_salt("left_panel_models_selector")
                .selected_text(&selected.name)
                .show_ui(ui, |ui| {
                    for model in &self.ollama_models.available {
                        let checked = selected.name == model.name;
                        if ui.selectable_label(checked, &model.name).clicked() {
                            action = Some(Action::SelectOllamaModel(model.clone()));
                        }
                    }
                });
        }

        action
    }

    fn show_left_panel_prompts(
        &mut self,
        ui: &mut egui::Ui,
        remove_prompt_modal: &Modal,
    ) -> Option<Action> {
        let mut action = None;

        ScrollArea::vertical().show(ui, |ui| {
            for (idx, prompt) in self.prompts.iter().enumerate() {
                let selected = self.is_prompt_selected(idx);

                ui.add_space(3.0);

                assign_if_some!(action, prompt.show_left_panel(ui, selected, idx));

                if remove_prompt_modal.was_outside_clicked() {
                    action = Some(Action::CloseRemovePromptDialog);
                }
            }

            remove_prompt_modal.show(|ui| {
                assign_if_some!(
                    action,
                    self.show_remove_prompt_modal(ui, remove_prompt_modal)
                );
            });
        });

        action
    }

    fn show_remove_prompt_modal(&mut self, ui: &mut egui::Ui, modal: &Modal) -> Option<Action> {
        let mut action = None;

        modal.title(ui, "Remove Prompt");
        modal.body_and_icon(
            ui,
            "Do you really want to remove this prompt?",
            Icon::Warning,
        );

        modal.buttons(ui, |ui| {
            if modal.button(ui, "Cancel").clicked() {
                action = Some(Action::CloseRemovePromptDialog);
            }

            if modal.caution_button(ui, "Remove").clicked() {
                match self.view_state.modal {
                    ViewModalState::RemovePrompt(idx) => {
                        action = Some(Action::RemovePrompt(idx));
                    }
                    _ => {
                        action = Some(Action::CloseRemovePromptDialog);
                    }
                }
            }
        });

        action
    }

    fn show_main_panel(&mut self, ctx: &egui::Context) {
        let Self {
            tokio_runtime,
            commonmark_cache,
            prompts,
            ollama_models,
            ..
        } = self;

        let local_model = match &ollama_models.selected {
            Some(model) => Some(model),
            None => self.ollama_models.available.first(),
        };
        let covered = !matches!(self.view_state.modal, ViewModalState::None);

        egui::CentralPanel::default().show(ctx, |ui| match self.view_state.main_panel {
            ViewMainPanelState::MixedHistory => {
                ui.label("Reprompt!");
            }
            ViewMainPanelState::Prompt(idx) => {
                if let Some(local_model) = local_model {
                    if let Some(prompt) = prompts.get_mut(idx) {
                        prompt.show_main_panel(
                            ui,
                            local_model,
                            covered,
                            tokio_runtime,
                            &self.ollama_client,
                            commonmark_cache,
                        );

                        if prompt.is_generating() {
                            ctx.request_repaint();
                        }
                    }
                }
            }
        });
    }

    fn show_add_prompt_modal(
        &mut self,
        ui: &mut egui::Ui,
        add_prompt_modal: &Modal,
    ) -> Option<Action> {
        let mut action = None;

        add_prompt_modal.frame(ui, |ui| {
            let text_width = Self::get_add_prompt_modal_width(ui.ctx());

            egui::TextEdit::singleline(&mut self.new_prompt_title)
                .hint_text("Write the title of your prompt here")
                .desired_width(text_width)
                .show(ui);

            egui::TextEdit::multiline(&mut self.new_prompt_content)
                .desired_rows(10)
                .hint_text(
                    "Write the content of your prompt here. It will be prepended to all requests.",
                )
                .desired_width(text_width)
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
                    action = Some(Action::CancelPromptCreation);
                }

                if ui.add(create_button).clicked()
                    && !self.new_prompt_title.is_empty()
                    && !self.new_prompt_content.is_empty()
                {
                    action = Some(Action::CreatePrompt);
                };
            },
        );

        action
    }

    fn is_prompt_selected(&self, idx: usize) -> bool {
        matches!(self.view_state.main_panel, ViewMainPanelState::Prompt(idx0) if idx0 == idx)
    }

    fn create_add_prompt_modal(ctx: &egui::Context, width: f32) -> Modal {
        let style = ModalStyle {
            default_width: Some(width),
            ..Default::default()
        };

        Modal::new(ctx, "add_prompt_modal")
            .with_close_on_outside_click(true)
            .with_style(&style)
    }

    fn get_add_prompt_modal_width(ctx: &egui::Context) -> f32 {
        ctx.available_rect().width() * 0.5
    }

    fn get_left_panel_width(ctx: &egui::Context) -> (f32, f32) {
        let available_width = ctx.available_rect().width();
        let max_width = available_width * 0.3;
        let min_width = available_width * 0.2;

        (max_width, min_width)
    }
}
