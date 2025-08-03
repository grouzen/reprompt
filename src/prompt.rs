use std::{collections::VecDeque, time::Instant};

use egui::{
    Color32, CornerRadius, Frame, Key, KeyboardShortcut, Label, Layout, Modifiers, ScrollArea,
    Sense, Stroke, UiBuilder,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use flowync::{CompactFlower, error::Compact};
use ollama_rs::models::LocalModel;
use tokio::runtime;

use crate::{app::AppAction, assign_if_some, ollama::OllamaClient};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Prompt {
    pub title: String,
    pub content: String,
    history: VecDeque<PromptResponse>,
    new_input: String,
    #[serde(skip)]
    ask_flower: PromptAskFlower,
    #[serde(skip)]
    pub state: PromptState,
}

impl Default for Prompt {
    fn default() -> Self {
        Self {
            title: Default::default(),
            content: Default::default(),
            history: Default::default(),
            new_input: Default::default(),
            ask_flower: PromptAskFlower::new(1),
            state: Default::default(),
        }
    }
}

type PromptAskFlower = CompactFlower<String, String, String>;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(default)]
struct PromptResponse {
    input: String,
    output: String,
    local_model_name: String,
    #[serde(skip)]
    requested_at: Instant,
}

impl Default for PromptResponse {
    fn default() -> Self {
        Self {
            input: Default::default(),
            output: Default::default(),
            local_model_name: "unknown_model".to_owned(),
            requested_at: Instant::now(),
        }
    }
}

#[derive(Default)]
pub enum PromptState {
    #[default]
    Idle,
    Generating,
}

impl PromptState {
    pub fn is_generating(&self) -> bool {
        matches!(self, PromptState::Generating)
    }
}

impl PromptResponse {
    pub fn new(input: String, output: String, local_model_name: String) -> Self {
        Self {
            input,
            output,
            local_model_name,
            ..Default::default()
        }
    }
}

impl Prompt {
    pub fn new(title: String, content: String, id: usize) -> Self {
        Self {
            title,
            content,
            ask_flower: PromptAskFlower::new(id),
            ..Default::default()
        }
    }

    pub fn remove_history(&mut self, history_idx: usize) {
        self.history.remove(history_idx);
    }

    pub fn show_left_panel(
        &self,
        ui: &mut egui::Ui,
        selected: bool,
        idx: usize,
    ) -> Option<AppAction> {
        let mut action = None;

        let response = ui.scope_builder(
            UiBuilder::new()
                .id_salt("left_panel_prompt")
                .sense(Sense::click()),
            |ui| {
                ui.with_layout(
                    Layout::left_to_right(egui::Align::TOP)
                        .with_main_justify(true)
                        .with_main_align(egui::Align::LEFT),
                    |ui| {
                        let fill_style = if selected {
                            ui.style().visuals.faint_bg_color
                        } else {
                            ui.style().visuals.window_fill
                        };
                        let stroke_style_color = if selected {
                            Color32::ORANGE
                        } else {
                            ui.style().visuals.window_stroke.color
                        };

                        Frame::group(ui.style())
                            .corner_radius(CornerRadius::same(6))
                            .stroke(Stroke::new(2.0, stroke_style_color))
                            .fill(fill_style)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let label_response =
                                        ui.add(egui::Label::wrap(egui::Label::new(&self.title)));

                                    ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                                        let remove_response = ui.add_enabled(
                                            !self.state.is_generating(),
                                            egui::Button::new("❌")
                                                .fill(Color32::TRANSPARENT)
                                                .small()
                                                .stroke(Stroke::NONE),
                                        );

                                        let edit_response = ui.add(
                                            egui::Button::new("\u{270f}")
                                                .fill(Color32::TRANSPARENT)
                                                .small()
                                                .stroke(Stroke::NONE),
                                        );

                                        if remove_response.on_hover_text("Remove prompt").clicked()
                                        {
                                            action = Some(AppAction::OpenRemovePromptDialog(idx));
                                        }

                                        if edit_response.on_hover_text("Edit prompt").clicked() {
                                            action = Some(AppAction::OpenEditPromptDialog(idx));
                                        }
                                    });

                                    label_response
                                })
                                .inner
                            })
                            .inner
                    },
                )
                .inner
            },
        );
        let response = response.response.union(response.inner);

        response
            .clone()
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        if response.clicked() {
            action = Some(AppAction::SelectPrompt(idx));
        }

        action
    }

    pub fn show_main_panel(
        &mut self,
        ui: &mut egui::Ui,
        is_modal_shown: bool,
        idx: usize,
        commonmark_cache: &mut CommonMarkCache,
    ) -> Option<AppAction> {
        let mut action = None;
        let is_input_interactive = !self.state.is_generating();

        ui.with_layout(
            Layout::left_to_right(egui::Align::TOP).with_main_justify(true),
            |ui| {
                egui::TextEdit::multiline(&mut self.new_input)
                    .hint_text(format!("Ask for the following prompt: {}", self.content))
                    .interactive(is_input_interactive)
                    .return_key(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter))
                    .show(ui);
            },
        );

        ui.separator();

        if is_input_interactive
            && !is_modal_shown
            && !self.new_input.is_empty()
            && ui.input(|i| i.key_pressed(Key::Enter) && i.modifiers.is_none())
        {
            action = Some(AppAction::GeneratePromptResponse {
                idx,
                input: self.new_input.clone(),
            });
        }

        if self.ask_flower.is_active() {
            assign_if_some!(action, self.poll_ask_flower());
        }

        assign_if_some!(action, self.show_prompt_history(ui, idx, commonmark_cache));

        action
    }

    fn show_prompt_history(
        &self,
        ui: &mut egui::Ui,
        idx: usize,
        commonmark_cache: &mut CommonMarkCache,
    ) -> Option<AppAction> {
        let mut action = None;

        ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            for (history_idx, prompt_response ) in self.history.iter().enumerate() {
                ui.add_space(3.0);
                ui.with_layout(
                    Layout::left_to_right(egui::Align::TOP)
                        .with_main_justify(true)
                        .with_main_align(egui::Align::LEFT),
                    |ui| {
                        Frame::group(ui.style())
                            .corner_radius(CornerRadius::same(6))
                            .stroke(Stroke::new(1.0, ui.style().visuals.window_stroke.color))
                            .show(ui, |ui| {
                                ui.with_layout(
                                    Layout::top_down(egui::Align::TOP)
                                        .with_cross_justify(true)
                                        .with_cross_align(egui::Align::LEFT),
                                    |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label("🖳");
                                            ui.label(&prompt_response.local_model_name);

                                            ui.with_layout(
                                                Layout::right_to_left(egui::Align::Min),
                                                |ui| {
                                                    let remove_response = ui.add_enabled(
                                                        !self.state.is_generating(),
                                                        egui::Button::new("❌")
                                                            .fill(Color32::TRANSPARENT)
                                                            .small()
                                                            .stroke(Stroke::NONE),
                                                    );

                                                    let regenerate_response = ui.add_enabled(
                                                        !self.state.is_generating(),
                                                        egui::Button::new("🔄")
                                                            .fill(Color32::TRANSPARENT)
                                                            .small()
                                                            .stroke(Stroke::NONE),
                                                    );

                                                    if remove_response
                                                        .on_hover_text("Remove from prompt history")
                                                        .clicked()
                                                    {
                                                        action = Some(AppAction::OpenRemovePromptHistoryDialog { idx, history_idx});
                                                    }

                                                    if regenerate_response
                                                        .on_hover_text("Regenerate with current model")
                                                        .clicked()
                                                    {
                                                        action = Some(AppAction::RegeneratePromptResponse { idx, history_idx });
                                                    }
                                                },
                                            );
                                        });

                                        ui.add_space(6.0);

                                        Frame::group(ui.style())
                                            .stroke(Stroke::new(
                                                1.0,
                                                ui.style().visuals.window_stroke.color,
                                            ))
                                            .fill(ui.style().visuals.faint_bg_color)
                                            .show(ui, |ui| {
                                                ui.add(egui::Label::wrap(Label::new(
                                                    &prompt_response.input,
                                                )));
                                            });

                                        CommonMarkViewer::new().show(
                                            ui,
                                            commonmark_cache,
                                            &prompt_response.output,
                                        );
                                    },
                                );
                            });
                    },
                );
            }
        });

        action
    }

    pub fn generate_response(
        &mut self,
        input: String,
        local_model: &LocalModel,
        rt: &runtime::Runtime,
        ollama_client: &OllamaClient,
    ) {
        self.state = PromptState::Generating;

        let response = PromptResponse::new(input.clone(), String::new(), local_model.name.clone());
        self.history.push_front(response);

        self.ask_ollama(input, local_model, rt, ollama_client.clone());
    }

    pub fn regenerate_response(
        &mut self,
        history_idx: usize,
        local_model: &LocalModel,
        rt: &runtime::Runtime,
        ollama_client: &OllamaClient,
    ) {
        if let Some(original_response) = self.history.get(history_idx) {
            let input = original_response.input.clone();
            self.generate_response(input, local_model, rt, ollama_client);
        }
    }

    fn ask_ollama(
        &self,
        question: String,
        local_model: &LocalModel,
        rt: &runtime::Runtime,
        ollama_client: OllamaClient,
    ) {
        let handle = self.ask_flower.handle();
        let prompt = format!("{}:\n{}", self.content, question);
        let local_model = local_model.clone();

        rt.spawn(async move {
            handle.activate();

            match ollama_client
                .generate_completion(prompt, &local_model, |response| handle.send(response))
                .await
            {
                Ok(response) => handle.success(response),
                Err(e) => handle.error(e.to_string()),
            }
        });
    }

    fn poll_ask_flower(&mut self) -> Option<AppAction> {
        let mut action = None;

        self.ask_flower
            .extract(|output| {
                self.history.get_mut(0).unwrap().output = output;
            })
            .finalize(|result| {
                match result {
                    Ok(output) => {
                        self.history.get_mut(0).unwrap().output = output;
                    }
                    Err(Compact::Suppose(e)) => {
                        // Remove the failed response from history
                        self.history.pop_front();

                        action = Some(AppAction::ShowErrorDialog {
                            title: "Response Generation Error".to_string(),
                            message: format!("Failed to generate response from Ollama. Please check your connection and try again.\n\nError: {e}"),
                        });
                    }
                    Err(Compact::Panicked(e)) => {
                        // Remove the failed response from history
                        self.history.pop_front();

                        action = Some(AppAction::ShowErrorDialog {
                            title: "Response Generation Error".to_string(),
                            message: format!("An unexpected error occurred while generating the response.\n\nError: {e}"),
                        });
                    }
                }

                self.state = PromptState::Idle;
                self.new_input.clear();
            });

        action
    }
}
