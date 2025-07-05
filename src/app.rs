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
    tokio_runtime: runtime::Runtime,
    #[serde(skip)]
    ollama_client: OllamaClient,
    #[serde(skip)]
    commonmark_cache: CommonMarkCache,
}

impl Default for RepromptApp {
    fn default() -> Self {
        Self {
            prompts: Vec::new(),
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
    AddPrompt {
        title: String,
        content: String,
    },
    RemovePrompt(usize),
    EditPrompt {
        idx: usize,
        title: String,
        content: String,
    },
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
enum ViewMainPanelState {
    #[default]
    MixedHistory,
    Prompt(usize),
}

impl ViewState {
    pub fn is_modal_shown(&self) -> bool {
        !matches!(self.modal, ViewModalState::None)
    }

    pub fn close_modal(&mut self) {
        self.modal = ViewModalState::None;
    }

    pub fn open_add_prompt_modal(&mut self) {
        self.modal = ViewModalState::AddPrompt {
            title: String::with_capacity(256),
            content: String::with_capacity(1024),
        };
    }

    pub fn get_add_prompt_modal_data_mut(&mut self) -> Option<(&mut String, &mut String)> {
        match &mut self.modal {
            ViewModalState::AddPrompt { title, content } => Some((title, content)),
            _ => None,
        }
    }

    pub fn get_add_prompt_modal_data(&self) -> Option<(&String, &String)> {
        match &self.modal {
            ViewModalState::AddPrompt { title, content } => Some((title, content)),
            _ => None,
        }
    }

    pub fn open_remove_prompt_modal(&mut self, idx: usize) {
        self.modal = ViewModalState::RemovePrompt(idx);
    }

    pub fn open_edit_prompt_modal(&mut self, idx: usize, prompt: &Prompt) {
        self.modal = ViewModalState::EditPrompt {
            idx,
            title: prompt.title.clone(),
            content: prompt.content.clone(),
        };
    }

    pub fn get_edit_prompt_modal_data_mut(&mut self) -> Option<(&mut String, &mut String)> {
        match &mut self.modal {
            ViewModalState::EditPrompt { title, content, .. } => Some((title, content)),
            _ => None,
        }
    }

    pub fn get_edit_prompt_modal_data(&self) -> Option<(usize, &String, &String)> {
        match &self.modal {
            ViewModalState::EditPrompt {
                idx,
                title,
                content,
            } => Some((*idx, title, content)),
            _ => None,
        }
    }

    pub fn select_prompt(&mut self, idx: usize) {
        self.main_panel = ViewMainPanelState::Prompt(idx);
    }

    fn is_prompt_selected(&self, idx: usize) -> bool {
        matches!(self.main_panel, ViewMainPanelState::Prompt(idx0) if idx0 == idx)
    }
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

impl Default for OllamaModels {
    fn default() -> Self {
        Self {
            selected: Default::default(),
            available: Default::default(),
            load_flower: LoadLocalModelsFlower::new(1),
        }
    }
}

type LoadLocalModelsFlower =
    CompactFlower<Vec<LocalModel>, (Vec<LocalModel>, Option<LocalModel>), anyhow::Error>;

#[derive(Debug)]
pub enum Action {
    OpenAddPromptDialog,
    CloseAddPromptDialog,
    CancelPromptModification,
    CreatePrompt,
    OpenRemovePromptDialog(usize),
    CloseRemovePromptDialog,
    RemovePrompt(usize),
    OpenEditPromptDialog(usize),
    CloseEditPromptDialog,
    EditPrompt,
    SelectPrompt(usize),
    SelectOllamaModel(LocalModel),
}

impl eframe::App for RepromptApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let add_prompt_modal = Self::create_add_edit_prompt_modal(
            ctx,
            "add_prompt_modal".to_string(),
            ctx.available_rect().width() * 0.5,
        );
        let edit_prompt_modal = Self::create_add_edit_prompt_modal(
            ctx,
            "edit_prompt_modal".to_string(),
            ctx.available_rect().width() * 0.5,
        );
        let remove_prompt_modal =
            Modal::new(ctx, "remove_prompt_modal").with_close_on_outside_click(true);

        let action = self.show(
            ctx,
            &add_prompt_modal,
            &remove_prompt_modal,
            &edit_prompt_modal,
        );

        self.handle_action(
            action,
            &add_prompt_modal,
            &remove_prompt_modal,
            &edit_prompt_modal,
        );
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

    fn remove_prompt(&mut self, idx: usize) {
        self.prompts.remove(idx);
    }

    fn add_prompt(&mut self, title: String, content: String) {
        let id = self.prompts.len();
        let prompt = Prompt::new(title, content, id);

        self.prompts.push(prompt);
    }

    fn edit_prompt(&mut self, idx: usize, title: String, content: String) {
        if let Some(prompt) = self.prompts.get_mut(idx) {
            prompt.title = title;
            prompt.content = content;
        }
    }

    fn handle_action(
        &mut self,
        action: Option<Action>,
        add_prompt_modal: &Modal,
        remove_prompt_modal: &Modal,
        edit_prompt_modal: &Modal,
    ) {
        if let Some(action) = action {
            match action {
                Action::OpenAddPromptDialog => {
                    add_prompt_modal.open();
                    self.view_state.open_add_prompt_modal();
                }
                Action::CloseAddPromptDialog => {
                    self.view_state.close_modal();
                }
                Action::CancelPromptModification => {
                    add_prompt_modal.close();
                    edit_prompt_modal.close();
                    self.view_state.close_modal();
                }
                Action::CreatePrompt => {
                    if let Some((title, content)) = self.view_state.get_add_prompt_modal_data() {
                        self.add_prompt(title.clone(), content.clone());

                        add_prompt_modal.close();
                        self.view_state.close_modal();
                    }
                }
                Action::OpenRemovePromptDialog(idx) => {
                    remove_prompt_modal.open();
                    self.view_state.open_remove_prompt_modal(idx);
                }
                Action::CloseRemovePromptDialog => {
                    self.view_state.close_modal();
                }
                Action::RemovePrompt(idx) => {
                    self.view_state.close_modal();
                    self.remove_prompt(idx);
                }
                Action::OpenEditPromptDialog(idx) => {
                    if let Some(prompt) = self.prompts.get(idx) {
                        edit_prompt_modal.open();
                        self.view_state.open_edit_prompt_modal(idx, prompt);
                    }
                }
                Action::CloseEditPromptDialog => {
                    self.view_state.close_modal();
                }
                Action::EditPrompt => {
                    if let Some((idx, title, content)) =
                        self.view_state.get_edit_prompt_modal_data()
                    {
                        self.edit_prompt(idx, title.clone(), content.clone());

                        edit_prompt_modal.close();
                        self.view_state.close_modal();
                    }
                }
                Action::SelectPrompt(idx) => {
                    self.view_state.select_prompt(idx);
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
        edit_prompt_modal: &Modal,
    ) -> Option<Action> {
        let action = self.show_left_panel(
            ctx,
            add_prompt_modal,
            remove_prompt_modal,
            edit_prompt_modal,
        );

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
        edit_prompt_modal: &Modal,
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
                    self.show_left_panel_prompts(ui, remove_prompt_modal, edit_prompt_modal)
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
        edit_prompt_modal: &Modal,
    ) -> Option<Action> {
        let mut action = None;

        ScrollArea::vertical().show(ui, |ui| {
            for (idx, prompt) in self.prompts.iter().enumerate() {
                let selected = self.view_state.is_prompt_selected(idx);

                ui.add_space(3.0);

                assign_if_some!(action, prompt.show_left_panel(ui, selected, idx));

                if remove_prompt_modal.was_outside_clicked() {
                    action = Some(Action::CloseRemovePromptDialog);
                }

                if edit_prompt_modal.was_outside_clicked() {
                    action = Some(Action::CloseEditPromptDialog);
                }
            }

            remove_prompt_modal.show(|ui| {
                assign_if_some!(
                    action,
                    self.show_remove_prompt_modal(ui, remove_prompt_modal)
                );
            });

            edit_prompt_modal.show(|ui| {
                assign_if_some!(action, self.show_edit_prompt_modal(ui, edit_prompt_modal))
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
                            self.view_state.is_modal_shown(),
                            tokio_runtime,
                            &self.ollama_client,
                            commonmark_cache,
                        );

                        if prompt.state.is_generating() {
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

        if let Some((title, content)) = self.view_state.get_add_prompt_modal_data_mut() {
            add_prompt_modal.frame(ui, |ui| {
                let text_width = Self::get_add_edit_prompt_modal_width(ui.ctx());

                egui::TextEdit::singleline(title)
                    .hint_text("Write the title of your prompt here")
                    .desired_width(text_width)
                    .show(ui);

                egui::TextEdit::multiline(content)
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
                    let cancel_button = Button::new(
                        WidgetText::from("Cancel").color(Color32::from_rgb(242, 148, 148)),
                    )
                    .fill(Color32::from_rgb(87, 38, 34));
                    let create_button = Button::new(
                        WidgetText::from("Create").color(Color32::from_rgb(141, 182, 242)),
                    )
                    .fill(Color32::from_rgb(33, 54, 84));

                    if ui.add(cancel_button).clicked() {
                        action = Some(Action::CancelPromptModification);
                    }

                    if ui.add(create_button).clicked() && !title.is_empty() && !content.is_empty() {
                        action = Some(Action::CreatePrompt);
                    };
                },
            );
        }

        action
    }

    fn show_edit_prompt_modal(
        &mut self,
        ui: &mut egui::Ui,
        edit_prompt_modal: &Modal,
    ) -> Option<Action> {
        let mut action = None;

        if let Some((title, content)) = self.view_state.get_edit_prompt_modal_data_mut() {
            edit_prompt_modal.frame(ui, |ui| {
                let text_width = Self::get_add_edit_prompt_modal_width(ui.ctx());

                egui::TextEdit::singleline(title)
                    .hint_text("Write the title of your prompt here")
                    .desired_width(text_width)
                    .show(ui);

                egui::TextEdit::multiline(content)
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
                    let cancel_button = Button::new(
                        WidgetText::from("Cancel").color(Color32::from_rgb(242, 148, 148)),
                    )
                    .fill(Color32::from_rgb(87, 38, 34));
                    let create_button = Button::new(
                        WidgetText::from("Create").color(Color32::from_rgb(141, 182, 242)),
                    )
                    .fill(Color32::from_rgb(33, 54, 84));

                    if ui.add(cancel_button).clicked() {
                        action = Some(Action::CancelPromptModification);
                    }

                    if ui.add(create_button).clicked() && !title.is_empty() && !content.is_empty() {
                        action = Some(Action::EditPrompt);
                    };
                },
            );
        }

        action
    }

    fn create_add_edit_prompt_modal(ctx: &egui::Context, id: String, width: f32) -> Modal {
        let style = ModalStyle {
            default_width: Some(width),
            ..Default::default()
        };

        Modal::new(ctx, id)
            .with_close_on_outside_click(true)
            .with_style(&style)
    }

    fn get_add_edit_prompt_modal_width(ctx: &egui::Context) -> f32 {
        ctx.available_rect().width() * 0.5
    }

    fn get_left_panel_width(ctx: &egui::Context) -> (f32, f32) {
        let available_width = ctx.available_rect().width();
        let max_width = available_width * 0.3;
        let min_width = available_width * 0.2;

        (max_width, min_width)
    }
}
