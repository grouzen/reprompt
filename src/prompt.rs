use std::{collections::VecDeque, time::Instant};

use egui::{
    CornerRadius, Frame, InnerResponse, Key, KeyboardShortcut, Label, Layout, Modifiers,
    ScrollArea, Sense, Stroke, UiBuilder,
};
use flowync::{CompactFlower, error::Compact};
use ollama_rs::{Ollama, generation::completion::request::GenerationRequest};
use tokio::runtime;
use tokio_stream::StreamExt;

const DEFAULT_OLLAMA_MODEL: &str = "qwen2.5:7b";

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
            requested_at: Instant::now(),
        }
    }
}

impl PromptResponse {
    pub fn new(input: String, output: String) -> Self {
        Self {
            input,
            output,
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

    pub fn show_left_panel(&self, ui: &mut egui::Ui, selected: bool, on_click: impl FnOnce()) {
        let InnerResponse {
            response: outer_response,
            inner: inner_response,
        } = ui.scope_builder(
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
                                ui.add(egui::Label::wrap(egui::Label::new(&self.title)))
                            })
                            .inner
                    },
                )
                .inner
            },
        );

        inner_response
            .clone()
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        if outer_response.clicked() || inner_response.clicked() {
            on_click();
        }
    }

    pub fn show_main_panel(
        &mut self,
        ui: &mut egui::Ui,
        covered: bool,
        rt: &runtime::Runtime,
        ollama: &Ollama,
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

        if !self.is_generating()
            && !covered
            && ui.input(|i| i.key_pressed(Key::Enter) && i.modifiers.is_none())
        {
            self.generate_response(self.new_input.clone(), rt, ollama);
        }

        if self.ask_flower.is_active() {
            self.poll_ask_flower();
        }

        self.show_prompt_history(ui);
    }

    fn show_prompt_history(&self, ui: &mut egui::Ui) {
        ScrollArea::vertical().show(ui, |ui| {
            for response in self.history.iter() {
                ui.with_layout(
                    Layout::left_to_right(egui::Align::TOP)
                        .with_main_justify(true)
                        .with_main_align(egui::Align::LEFT),
                    |ui| {
                        Frame::group(ui.style())
                            .corner_radius(CornerRadius::same(6))
                            .stroke(Stroke::new(1.0, ui.style().visuals.window_stroke.color))
                            .show(ui, |ui| {
                                ui.add(egui::Label::wrap(Label::new(&response.output)));
                            });
                    },
                );
            }
        });
    }

    pub fn is_generating(&self) -> bool {
        matches!(self.state, PromptState::Generating)
    }

    fn generate_response(&mut self, input: String, rt: &runtime::Runtime, ollama: &Ollama) {
        self.state = PromptState::Generating;

        let response = PromptResponse::new(self.new_input.clone(), String::new());
        self.history.push_front(response);

        self.ask_ollama(input, rt, ollama.clone());
    }

    fn ask_ollama(&self, question: String, rt: &runtime::Runtime, ollama: Ollama) {
        let handle = self.ask_flower.handle();
        let prompt = format!("{}:\n{}", self.content, question);

        rt.spawn(async move {
            handle.activate();

            match Self::generate_ollama_completion(prompt, ollama, |response| handle.send(response))
                .await
            {
                Ok(response) => handle.success(response),
                Err(e) => handle.error(e.to_string()),
            }
        });
    }

    // TODO: move to Ollama client
    async fn generate_ollama_completion(
        prompt: String,
        ollama: Ollama,
        on_next: impl Fn(String),
    ) -> anyhow::Result<String> {
        let mut stream = ollama
            .generate_stream(GenerationRequest::new(DEFAULT_OLLAMA_MODEL.into(), prompt))
            .await?;
        let mut response = String::new();

        while let Some(Ok(next)) = stream.next().await {
            for n in next {
                response += &n.response;
                on_next(response.clone());
            }
        }

        Ok(response)
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
                        let message = format!("Tokio task panicked: {}", e);
                        self.history.get_mut(0).unwrap().output = message;
                    }
                }

                self.state = PromptState::Idle;
                self.new_input.clear();
            });
    }
}
