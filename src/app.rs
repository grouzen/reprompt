use egui::{Color32, Layout, ScrollArea, Stroke};
use egui_commonmark::CommonMarkCache;
use egui_modal::Modal;
use egui_theme_switch::global_theme_switch;
use flowync::{CompactFlower, error::Compact};
use ollama_rs::{Ollama, models::LocalModel};
use tokio::runtime;

use crate::{
    assign_if_some,
    ollama::OllamaClient,
    prompt::Prompt,
    view::{View, ViewMainPanel},
};

pub const TITLE: &str = "Reprompt";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct App {
    prompts: Vec<Prompt>,
    view: View,
    ollama_models: OllamaModels,
    #[serde(skip)]
    tokio_runtime: runtime::Runtime,
    #[serde(skip)]
    ollama_client: OllamaClient,
    #[serde(skip)]
    commonmark_cache: CommonMarkCache,
}

impl Default for App {
    fn default() -> Self {
        Self {
            prompts: Vec::new(),
            view: Default::default(),
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
pub enum AppAction {
    GeneratePromptResponse { idx: usize, input: String },
    RegeneratePromptResponse { idx: usize, history_idx: usize },
    CloseDialog,
    OpenAddPromptDialog,
    CancelPromptModification,
    CreatePrompt,
    OpenRemovePromptDialog(usize),
    RemovePrompt(usize),
    OpenEditPromptDialog(usize),
    OpenRemovePromptHistoryDialog { idx: usize, history_idx: usize },
    RemovePromptHistory { idx: usize, history_idx: usize },
    EditPrompt,
    SelectPrompt(usize),
    SelectOllamaModel(LocalModel),
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let add_prompt_modal = View::create_modify_prompt_modal(
            ctx,
            "add_prompt_modal".to_string(),
            ctx.available_rect().width() * 0.5,
        );
        let edit_prompt_modal = View::create_modify_prompt_modal(
            ctx,
            "edit_prompt_modal".to_string(),
            ctx.available_rect().width() * 0.5,
        );
        let remove_prompt_modal =
            Modal::new(ctx, "remove_prompt_modal").with_close_on_outside_click(true);
        let remove_prompt_history_modal =
            Modal::new(ctx, "remove_prompt_history_modal").with_close_on_outside_click(true);

        let action = self.show(
            ctx,
            &add_prompt_modal,
            &remove_prompt_modal,
            &edit_prompt_modal,
            &remove_prompt_history_modal,
        );

        self.handle_action(
            action,
            &add_prompt_modal,
            &remove_prompt_modal,
            &edit_prompt_modal,
            &remove_prompt_history_modal,
        );
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}

impl App {
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

    fn get_prompt_mut(&mut self, idx: usize) -> Option<&mut Prompt> {
        self.prompts.get_mut(idx)
    }

    fn handle_action(
        &mut self,
        action: Option<AppAction>,
        add_prompt_modal: &Modal,
        remove_prompt_modal: &Modal,
        edit_prompt_modal: &Modal,
        remove_prompt_history_modal: &Modal,
    ) {
        if let Some(action) = action {
            match action {
                AppAction::GeneratePromptResponse { idx, input } => {
                    if let Some(selected_model) = &self.ollama_models.selected {
                        if let Some(prompt) = self.prompts.get_mut(idx) {
                            prompt.generate_response(
                                input,
                                selected_model,
                                &self.tokio_runtime,
                                &self.ollama_client,
                            );
                        }
                    }
                }
                AppAction::RegeneratePromptResponse { idx, history_idx } => {
                    if let Some(selected_model) = &self.ollama_models.selected {
                        if let Some(prompt) = self.prompts.get_mut(idx) {
                            prompt.regenerate_response(
                                history_idx,
                                selected_model,
                                &self.tokio_runtime,
                                &self.ollama_client,
                            );
                        }
                    }
                }
                AppAction::CloseDialog => {
                    self.view.close_modal();
                }
                AppAction::OpenAddPromptDialog => {
                    add_prompt_modal.open();
                    self.view.open_add_prompt_modal();
                }
                AppAction::CancelPromptModification => {
                    add_prompt_modal.close();
                    edit_prompt_modal.close();
                    self.view.close_modal();
                }
                AppAction::CreatePrompt => {
                    if let Some((title, content)) = self.view.get_add_prompt_modal_data() {
                        self.add_prompt(title.clone(), content.clone());

                        add_prompt_modal.close();
                        self.view.close_modal();
                    }
                }
                AppAction::OpenRemovePromptDialog(idx) => {
                    remove_prompt_modal.open();
                    self.view.open_remove_prompt_modal(idx);
                }
                AppAction::RemovePrompt(idx) => {
                    self.view.close_modal();
                    self.remove_prompt(idx);
                }
                AppAction::OpenEditPromptDialog(idx) => {
                    if let Some(prompt) = self.prompts.get(idx) {
                        edit_prompt_modal.open();
                        self.view.open_edit_prompt_modal(idx, prompt);
                    }
                }
                AppAction::OpenRemovePromptHistoryDialog { idx, history_idx } => {
                    remove_prompt_history_modal.open();
                    self.view.open_remove_prompt_history_modal(idx, history_idx);
                }
                AppAction::RemovePromptHistory { idx, history_idx } => {
                    self.view.close_modal();

                    if let Some(prompt) = self.get_prompt_mut(idx) {
                        prompt.remove_history(history_idx);
                    }
                }
                AppAction::EditPrompt => {
                    if let Some((idx, title, content)) = self.view.get_edit_prompt_modal_data() {
                        self.edit_prompt(idx, title.clone(), content.clone());

                        edit_prompt_modal.close();
                        self.view.close_modal();
                    }
                }
                AppAction::SelectPrompt(idx) => {
                    self.view.select_prompt(idx);
                }
                AppAction::SelectOllamaModel(local_model) => {
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
        remove_prompt_history_modal: &Modal,
    ) -> Option<AppAction> {
        let mut action = None;

        assign_if_some!(
            action,
            self.show_left_panel(
                ctx,
                add_prompt_modal,
                remove_prompt_modal,
                edit_prompt_modal,
            )
        );

        assign_if_some!(
            action,
            self.show_main_panel(ctx, remove_prompt_history_modal)
        );

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
    ) -> Option<AppAction> {
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
                    action = Some(AppAction::CloseDialog);
                }

                add_prompt_modal.show(|ui| {
                    assign_if_some!(
                        action,
                        self.view.show_add_prompt_modal(ui, add_prompt_modal)
                    );
                });
            });

        action
    }

    fn show_left_panel_create_protmp_button(&mut self, ui: &mut egui::Ui) -> Option<AppAction> {
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
            action = Some(AppAction::OpenAddPromptDialog);
        }

        action
    }

    fn show_left_panel_model_selector(&mut self, ui: &mut egui::Ui) -> Option<AppAction> {
        let mut action = None;

        if let Some(selected) = &self.ollama_models.selected {
            egui::ComboBox::from_id_salt("left_panel_models_selector")
                .selected_text(&selected.name)
                .show_ui(ui, |ui| {
                    for model in &self.ollama_models.available {
                        let checked = selected.name == model.name;
                        if ui.selectable_label(checked, &model.name).clicked() {
                            action = Some(AppAction::SelectOllamaModel(model.clone()));
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
    ) -> Option<AppAction> {
        let mut action = None;

        ScrollArea::vertical().show(ui, |ui| {
            for (idx, prompt) in self.prompts.iter().enumerate() {
                let selected = self.view.is_prompt_selected(idx);

                ui.add_space(3.0);

                assign_if_some!(action, prompt.show_left_panel(ui, selected, idx));

                if remove_prompt_modal.was_outside_clicked()
                    || edit_prompt_modal.was_outside_clicked()
                {
                    action = Some(AppAction::CloseDialog);
                }
            }

            remove_prompt_modal.show(|ui| {
                assign_if_some!(
                    action,
                    self.view.show_remove_prompt_modal(ui, remove_prompt_modal)
                );
            });

            edit_prompt_modal.show(|ui| {
                assign_if_some!(
                    action,
                    self.view.show_edit_prompt_modal(ui, edit_prompt_modal)
                )
            });
        });

        action
    }

    fn show_main_panel(
        &mut self,
        ctx: &egui::Context,
        remove_prompt_history_modal: &Modal,
    ) -> Option<AppAction> {
        let mut action = None;

        let Self {
            commonmark_cache,
            prompts,
            ..
        } = self;

        egui::CentralPanel::default().show(ctx, |ui| match self.view.main_panel {
            ViewMainPanel::Welcome => {
                ui.add_space(20.0);
                ui.with_layout(Layout::top_down(egui::Align::Center), |ui| {
                    ui.label("Welcome to the Reprompt app! Please select a model and add prompts to get started.");
                });
            }
            ViewMainPanel::Prompt(idx) => {
                if let Some(prompt) = prompts.get_mut(idx) {
                    assign_if_some!(action, prompt.show_main_panel(
                        ui,
                        self.view.is_modal_shown(),
                        idx,
                        commonmark_cache,
                    ));

                    if remove_prompt_history_modal.was_outside_clicked() {
                        action = Some(AppAction::CloseDialog);
                    }

                    remove_prompt_history_modal.show(|ui| {
                        assign_if_some!(action, self.view.show_remove_prompt_history_modal(ui, remove_prompt_history_modal));
                    });


                    if prompt.state.is_generating() {
                        ctx.request_repaint();
                    }
                }
            }
        });

        action
    }

    fn get_left_panel_width(ctx: &egui::Context) -> (f32, f32) {
        let available_width = ctx.available_rect().width();
        let max_width = available_width * 0.3;
        let min_width = available_width * 0.2;

        (max_width, min_width)
    }
}
