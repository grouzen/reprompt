use std::{collections::VecDeque, time::Instant};

use egui::{
    Color32, CornerRadius, Frame, Key, KeyboardShortcut, Label, Layout, Modifiers, ScrollArea,
    Sense, Stroke, UiBuilder,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use flowync::{CompactFlower, error::Compact};
use ollama_rs::models::LocalModel;
use tokio::runtime;

use crate::{app::Action, ollama::OllamaClient};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Prompt {
    title: String,
    content: String,
    history: VecDeque<PromptResponse>,
    new_input: String,
    #[serde(skip)]
    ask_flower: PromptAskFlower,
    #[serde(skip)]
    state: PromptState,
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

#[derive(Default)]
enum PromptState {
    #[default]
    Idle,
    Generating,
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

    pub fn show_left_panel(&self, ui: &mut egui::Ui, selected: bool, idx: usize) -> Option<Action> {
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

                        Frame::group(ui.style())
                            .corner_radius(CornerRadius::same(6))
                            .stroke(Stroke::new(2.0, ui.style().visuals.window_stroke.color))
                            .fill(fill_style)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let label_response =
                                        ui.add(egui::Label::wrap(egui::Label::new(&self.title)));

                                    ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                                        let response = ui.add(
                                            egui::Button::new("❌")
                                                .fill(Color32::TRANSPARENT)
                                                .small()
                                                .stroke(Stroke::NONE),
                                        );

                                        if response.clone().on_hover_text("Remove prompt").clicked()
                                        {
                                            action = Some(Action::OpenRemovePromptDialog(idx));
                                        };

                                        response
                                    })
                                    .inner;

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
            action = Some(Action::SelectPrompt(idx));
        }

        action
    }

    pub fn show_main_panel(
        &mut self,
        ui: &mut egui::Ui,
        local_model: &LocalModel,
        covered: bool,
        rt: &runtime::Runtime,
        ollama_client: &OllamaClient,
        commonmark_cache: &mut CommonMarkCache,
    ) {
        ui.with_layout(
            Layout::left_to_right(egui::Align::TOP).with_main_justify(true),
            |ui| {
                let interactive = !self.is_generating();

                egui::TextEdit::multiline(&mut self.new_input)
                    .hint_text(format!("Ask for the following prompt: {}", self.content))
                    .interactive(interactive)
                    .return_key(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter))
                    .show(ui);
            },
        );

        ui.separator();

        if !self.is_generating()
            && !covered
            && !self.new_input.is_empty()
            && ui.input(|i| i.key_pressed(Key::Enter) && i.modifiers.is_none())
        {
            self.generate_response(self.new_input.clone(), local_model, rt, ollama_client);
        }

        if self.ask_flower.is_active() {
            self.poll_ask_flower();
        }

        self.show_prompt_history(ui, commonmark_cache);
    }

    fn show_prompt_history(&self, ui: &mut egui::Ui, commonmark_cache: &mut CommonMarkCache) {
        ScrollArea::vertical().show(ui, |ui| {
            for prompt_response in self.history.iter() {
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
    }

    pub fn is_generating(&self) -> bool {
        matches!(self.state, PromptState::Generating)
    }

    fn generate_response(
        &mut self,
        input: String,
        local_model: &LocalModel,
        rt: &runtime::Runtime,
        ollama_client: &OllamaClient,
    ) {
        self.state = PromptState::Generating;

        let response = PromptResponse::new(
            self.new_input.clone(),
            String::new(),
            local_model.name.clone(),
        );
        self.history.push_front(response);

        self.ask_ollama(input, local_model, rt, ollama_client.clone());
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

    fn poll_ask_flower(&mut self) {
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
                        self.history.get_mut(0).unwrap().output = e;
                    }
                    Err(Compact::Panicked(e)) => {
                        let message = format!("Tokio task panicked: {e}");
                        self.history.get_mut(0).unwrap().output = message;
                    }
                }

                self.state = PromptState::Idle;
                self.new_input.clear();
            });
    }
}
