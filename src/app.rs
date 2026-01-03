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
    prompt::{Prompt, PromptState},
    view::{View, ViewMainPanel},
};

pub const TITLE: &str = "Reprompt";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    HistoryCount,
    LastUsage,
    InsertionOrder,
}

impl Default for SortMode {
    fn default() -> Self {
        Self::InsertionOrder
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct App {
    prompts: Vec<Prompt>,
    view: View,
    ollama_models: OllamaModels,
    ui_scale: f32,
    #[serde(skip)]
    tokio_runtime: runtime::Runtime,
    #[serde(skip)]
    ollama_client: OllamaClient,
    #[serde(skip)]
    commonmark_cache: CommonMarkCache,
    #[serde(skip)]
    sort_mode: SortMode,
}

impl Default for App {
    fn default() -> Self {
        Self {
            prompts: Vec::new(),
            view: Default::default(),
            ui_scale: 1.2,
            tokio_runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
            ollama_client: OllamaClient::new(Ollama::default()),
            ollama_models: Default::default(),
            commonmark_cache: CommonMarkCache::default(),
            sort_mode: SortMode::InsertionOrder,
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
    StopPromptGeneration(usize),
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
    ReloadOllamaModels,
    SetUIScale(f32),
    ShowErrorDialog { title: String, message: String },
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut action = None;

        assign_if_some!(action, self.handle_keyboard_input(ctx));

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
        let error_modal = Modal::new(ctx, "error_modal").with_close_on_outside_click(true);

        assign_if_some!(
            action,
            self.show(
                ctx,
                &add_prompt_modal,
                &remove_prompt_modal,
                &edit_prompt_modal,
                &remove_prompt_history_modal,
                &error_modal,
            )
        );

        self.handle_action(
            action,
            ctx,
            &add_prompt_modal,
            &remove_prompt_modal,
            &edit_prompt_modal,
            &remove_prompt_history_modal,
            &error_modal,
        );
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }
}

impl App {
    pub fn from_eframe_context(cc: &eframe::CreationContext<'_>) -> Self {
        eframe::storage_dir(TITLE);

        let app: Self = match cc.storage {
            Some(storage) => eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default(),
            None => Default::default(),
        };

        Self::set_scale(&cc.egui_ctx, app.ui_scale);

        app
    }

    fn handle_keyboard_input(&self, ctx: &egui::Context) -> Option<AppAction> {
        let mut action = None;
        ctx.input(|i| {
            if i.modifiers.ctrl {
                if i.key_pressed(egui::Key::Equals) || i.key_pressed(egui::Key::Plus) {
                    // Ctrl+Plus: Increase scale by 0.1, clamped to max 2.5
                    let new_scale = (self.ui_scale + 0.1).min(2.5);
                    if new_scale != self.ui_scale {
                        action = Some(AppAction::SetUIScale(new_scale));
                    }
                } else if i.key_pressed(egui::Key::Minus) {
                    // Ctrl+Minus: Decrease scale by 0.1, clamped to min 1.0
                    let new_scale = (self.ui_scale - 0.1).max(1.0);
                    if new_scale != self.ui_scale {
                        action = Some(AppAction::SetUIScale(new_scale));
                    }
                }
            }
        });
        action
    }

    fn set_scale(ctx: &egui::Context, ui_scale: f32) {
        ctx.set_zoom_factor(ui_scale);
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

    #[allow(clippy::too_many_arguments)]
    fn handle_action(
        &mut self,
        action: Option<AppAction>,
        ctx: &egui::Context,
        add_prompt_modal: &Modal,
        remove_prompt_modal: &Modal,
        edit_prompt_modal: &Modal,
        remove_prompt_history_modal: &Modal,
        error_modal: &Modal,
    ) {
        if let Some(action) = action {
            match action {
                AppAction::StopPromptGeneration(idx) => {
                    if let Some(prompt) = self.prompts.get_mut(idx) {
                        self.ollama_client.cancel_generation();
                        prompt.state = PromptState::Idle;
                    }
                }
                AppAction::GeneratePromptResponse { idx, input } => {
                    if let Some(selected_model) = &self.ollama_models.selected
                        && let Some(prompt) = self.prompts.get_mut(idx)
                    {
                        prompt.generate_response(
                            input,
                            selected_model,
                            &self.tokio_runtime,
                            &self.ollama_client,
                        );
                    }
                }
                AppAction::RegeneratePromptResponse { idx, history_idx } => {
                    if let Some(selected_model) = &self.ollama_models.selected
                        && let Some(prompt) = self.prompts.get_mut(idx)
                    {
                        prompt.regenerate_response(
                            history_idx,
                            selected_model,
                            &self.tokio_runtime,
                            &self.ollama_client,
                        );
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
                AppAction::SetUIScale(scale) => {
                    self.ui_scale = scale;
                    Self::set_scale(ctx, scale);
                }
                AppAction::ShowErrorDialog { title, message } => {
                    error_modal.open();
                    self.view.open_error_modal(title, message);
                }
                AppAction::ReloadOllamaModels => {
                    self.load_local_models();
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

    fn poll_load_flower(&mut self) -> Option<AppAction> {
        let mut action = None;

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
                Err(Compact::Suppose(e)) => {
                    action = Some(AppAction::ShowErrorDialog {
                        title: "Model Loading Error".to_string(),
                        message: format!("Unable to load available models from Ollama. Please ensure Ollama is running and accessible.\n\nError: {e}"),
                    });
                }
                Err(Compact::Panicked(e)) => {
                    action = Some(AppAction::ShowErrorDialog {
                        title: "Model Loading Error".to_string(),
                        message: format!("An unexpected error occurred while loading models.\n\nError: {e}"),
                    });
                }
            });

        action
    }

    fn show(
        &mut self,
        ctx: &egui::Context,
        add_prompt_modal: &Modal,
        remove_prompt_modal: &Modal,
        edit_prompt_modal: &Modal,
        remove_prompt_history_modal: &Modal,
        error_modal: &Modal,
    ) -> Option<AppAction> {
        let mut action = None;

        assign_if_some!(
            action,
            self.show_left_panel(
                ctx,
                add_prompt_modal,
                remove_prompt_modal,
                edit_prompt_modal,
                error_modal,
            )
        );

        assign_if_some!(
            action,
            self.show_main_panel(ctx, remove_prompt_history_modal)
        );

        if self.ollama_models.load_flower.is_active() {
            assign_if_some!(action, self.poll_load_flower());
        }

        action
    }

    fn show_left_panel(
        &mut self,
        ctx: &egui::Context,
        add_prompt_modal: &Modal,
        remove_prompt_modal: &Modal,
        edit_prompt_modal: &Modal,
        error_modal: &Modal,
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

                    assign_if_some!(action, self.show_left_panel_model_selector(ui));

                    self.show_left_panel_sort_mode_selector(ui);
                });

                ui.separator();

                assign_if_some!(
                    action,
                    self.show_left_panel_prompts(ui, remove_prompt_modal, edit_prompt_modal)
                );

                ui.with_layout(Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        global_theme_switch(ui);

                        ui.add_space(6.0);

                        // UI Scale control
                        ui.horizontal(|ui| {
                            let mut scale = self.ui_scale;
                            if ui
                                .add(
                                    egui::Slider::new(&mut scale, 1.0..=2.5)
                                        .step_by(0.1)
                                        .show_value(false),
                                )
                                .on_hover_text("Use Ctrl- and Ctrl+ hotkeys")
                                .changed()
                            {
                                action = Some(AppAction::SetUIScale(scale));
                            }
                            ui.label(format!("{:.0}%", scale * 100.0));
                        });
                    });

                    ui.add_space(6.0);

                    // Version label (slightly smaller than default)
                    ui.label(egui::RichText::new(format!("v{VERSION}")).size(12.0));
                });

                if add_prompt_modal.was_outside_clicked() || error_modal.was_outside_clicked() {
                    action = Some(AppAction::CloseDialog);
                }

                add_prompt_modal.show(|ui| {
                    assign_if_some!(
                        action,
                        self.view.show_add_prompt_modal(ui, add_prompt_modal)
                    );
                });

                error_modal.show(|ui| {
                    assign_if_some!(action, self.view.show_error_modal(ui, error_modal));
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
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Create new prompt")
            .clicked()
        {
            action = Some(AppAction::OpenAddPromptDialog);
        }

        action
    }

    fn show_left_panel_model_selector(&mut self, ui: &mut egui::Ui) -> Option<AppAction> {
        let mut action = None;

        ui.horizontal(|ui| {
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

            // Add reload button next to the model selector
            if ui
                .add(
                    egui::Button::new("🔄")
                        .fill(Color32::TRANSPARENT)
                        .small()
                        .stroke(Stroke::NONE),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Reload models")
                .clicked()
            {
                action = Some(AppAction::ReloadOllamaModels);
            }
        });

        action
    }

    fn show_left_panel_sort_mode_selector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Sort by:");

            egui::ComboBox::from_id_salt("sort_mode_selector")
                .selected_text(match self.sort_mode {
                    SortMode::InsertionOrder => "insertion",
                    SortMode::HistoryCount => "history count",
                    SortMode::LastUsage => "last usage",
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(
                            matches!(self.sort_mode, SortMode::InsertionOrder),
                            "insertion (default)",
                        )
                        .clicked()
                    {
                        self.sort_mode = SortMode::InsertionOrder;
                    }
                    if ui
                        .selectable_label(
                            matches!(self.sort_mode, SortMode::HistoryCount),
                            "history count",
                        )
                        .clicked()
                    {
                        self.sort_mode = SortMode::HistoryCount;
                    }
                    if ui
                        .selectable_label(
                            matches!(self.sort_mode, SortMode::LastUsage),
                            "last usage",
                        )
                        .clicked()
                    {
                        self.sort_mode = SortMode::LastUsage;
                    }
                });
        });
    }

    /// Sorts prompt indices based on the current sort mode
    fn sort_prompt_indices(&self) -> Vec<usize> {
        let mut prompt_indices = (0..self.prompts.len()).collect::<Vec<usize>>();

        match self.sort_mode {
            SortMode::HistoryCount => {
                prompt_indices.sort_by(|&a, &b| {
                    // First sort by history count (descending)
                    let count_a = self.prompts[a].history_count();
                    let count_b = self.prompts[b].history_count();

                    count_b.cmp(&count_a)
                });
            }
            SortMode::LastUsage => {
                prompt_indices.sort_by(|&a, &b| {
                    // Sort by last usage time (descending)
                    let last_used_a = self.prompts[a].get_last_used_time();
                    let last_used_b = self.prompts[b].get_last_used_time();

                    // Handle cases where one or both might be None
                    match (last_used_a, last_used_b) {
                        (Some(time_a), Some(time_b)) => time_b.cmp(&time_a), // More recent first
                        (Some(_), None) => std::cmp::Ordering::Less,         // a is more recent
                        (None, Some(_)) => std::cmp::Ordering::Greater,      // b is more recent
                        (None, None) => std::cmp::Ordering::Equal,           // both are equal
                    }
                });
            }
            SortMode::InsertionOrder => {
                // No sorting - maintain insertion order
            }
        }

        prompt_indices
    }

    fn show_left_panel_prompts(
        &mut self,
        ui: &mut egui::Ui,
        remove_prompt_modal: &Modal,
        edit_prompt_modal: &Modal,
    ) -> Option<AppAction> {
        let mut action = None;

        ScrollArea::vertical().show(ui, |ui| {
            // Sort prompts based on current sort mode
            let prompt_indices = self.sort_prompt_indices();

            for &idx in &prompt_indices {
                let prompt = &self.prompts[idx];
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
