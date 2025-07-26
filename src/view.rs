use egui::{Button, Color32, Layout, WidgetText};
use egui_modal::{Icon, Modal, ModalStyle};

use crate::{app::AppAction, assign_if_some, prompt::Prompt};

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
pub struct View {
    modal: ViewModal,
    pub main_panel: ViewMainPanel,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
enum ViewModal {
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
    RemovePromptHistory {
        idx: usize,
        history_idx: usize,
    },
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub enum ViewMainPanel {
    #[default]
    Welcome,
    Prompt(usize),
}

impl View {
    pub fn is_modal_shown(&self) -> bool {
        !matches!(self.modal, ViewModal::None)
    }

    pub fn close_modal(&mut self) {
        self.modal = ViewModal::None;
    }

    pub fn open_add_prompt_modal(&mut self) {
        self.modal = ViewModal::AddPrompt {
            title: String::with_capacity(256),
            content: String::with_capacity(1024),
        };
    }

    pub fn get_add_prompt_modal_data_mut(&mut self) -> Option<(&mut String, &mut String)> {
        match &mut self.modal {
            ViewModal::AddPrompt { title, content } => Some((title, content)),
            _ => None,
        }
    }

    pub fn get_add_prompt_modal_data(&self) -> Option<(&String, &String)> {
        match &self.modal {
            ViewModal::AddPrompt { title, content } => Some((title, content)),
            _ => None,
        }
    }

    pub fn open_remove_prompt_modal(&mut self, idx: usize) {
        self.modal = ViewModal::RemovePrompt(idx);
    }

    pub fn open_edit_prompt_modal(&mut self, idx: usize, prompt: &Prompt) {
        self.modal = ViewModal::EditPrompt {
            idx,
            title: prompt.title.clone(),
            content: prompt.content.clone(),
        };
    }

    pub fn get_edit_prompt_modal_data_mut(&mut self) -> Option<(&mut String, &mut String)> {
        match &mut self.modal {
            ViewModal::EditPrompt { title, content, .. } => Some((title, content)),
            _ => None,
        }
    }

    pub fn get_edit_prompt_modal_data(&self) -> Option<(usize, &String, &String)> {
        match &self.modal {
            ViewModal::EditPrompt {
                idx,
                title,
                content,
            } => Some((*idx, title, content)),
            _ => None,
        }
    }

    pub fn open_remove_prompt_history_modal(&mut self, idx: usize, history_idx: usize) {
        self.modal = ViewModal::RemovePromptHistory { idx, history_idx };
    }

    pub fn select_prompt(&mut self, idx: usize) {
        self.main_panel = ViewMainPanel::Prompt(idx);
    }

    pub fn is_prompt_selected(&self, idx: usize) -> bool {
        matches!(self.main_panel, ViewMainPanel::Prompt(idx0) if idx0 == idx)
    }

    pub fn show_edit_prompt_modal(
        &mut self,
        ui: &mut egui::Ui,
        edit_prompt_modal: &Modal,
    ) -> Option<AppAction> {
        let mut action = None;

        if let Some((title, content)) = self.get_edit_prompt_modal_data_mut() {
            assign_if_some!(
                action,
                Self::show_modify_prompt_modal(
                    ui,
                    edit_prompt_modal,
                    title,
                    content,
                    AppAction::EditPrompt,
                    "Edit",
                )
            );
        }

        action
    }

    fn show_modify_prompt_modal(
        ui: &mut egui::Ui,
        modal: &Modal,
        title: &mut String,
        content: &mut String,
        ok_action: AppAction,
        ok_button_name: &str,
    ) -> Option<AppAction> {
        let mut action = None;

        modal.frame(ui, |ui| {
            let text_width = Self::get_modify_prompt_modal_width(ui.ctx());

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
                let cancel_button =
                    Button::new(WidgetText::from("Cancel").color(Color32::from_rgb(242, 148, 148)))
                        .fill(Color32::from_rgb(87, 38, 34));
                let ok_button = Button::new(
                    WidgetText::from(ok_button_name).color(Color32::from_rgb(141, 182, 242)),
                )
                .fill(Color32::from_rgb(33, 54, 84));

                if ui.add(cancel_button).clicked() {
                    action = Some(AppAction::CancelPromptModification);
                }

                if ui.add(ok_button).clicked() && !title.is_empty() && !content.is_empty() {
                    action = Some(ok_action);
                };
            },
        );

        action
    }

    pub fn show_add_prompt_modal(
        &mut self,
        ui: &mut egui::Ui,
        add_prompt_modal: &Modal,
    ) -> Option<AppAction> {
        let mut action = None;

        if let Some((title, content)) = self.get_add_prompt_modal_data_mut() {
            assign_if_some!(
                action,
                Self::show_modify_prompt_modal(
                    ui,
                    add_prompt_modal,
                    title,
                    content,
                    AppAction::CreatePrompt,
                    "Create",
                )
            );
        }

        action
    }

    pub fn show_remove_prompt_modal(&self, ui: &mut egui::Ui, modal: &Modal) -> Option<AppAction> {
        let mut action = None;

        modal.title(ui, "Remove Prompt");
        modal.body_and_icon(
            ui,
            "Do you really want to remove this prompt?",
            Icon::Warning,
        );

        modal.buttons(ui, |ui| {
            if modal.button(ui, "Cancel").clicked() {
                action = Some(AppAction::CloseDialog);
            }

            if modal.caution_button(ui, "Remove").clicked() {
                match self.modal {
                    ViewModal::RemovePrompt(idx) => {
                        action = Some(AppAction::RemovePrompt(idx));
                    }
                    _ => {
                        action = Some(AppAction::CloseDialog);
                    }
                }
            }
        });

        action
    }

    pub fn show_remove_prompt_history_modal(
        &self,
        ui: &mut egui::Ui,
        modal: &Modal,
    ) -> Option<AppAction> {
        let mut action = None;

        modal.title(ui, "Remove prompt history");
        modal.body_and_icon(
            ui,
            "Do you really want to remove this prompt history item?",
            Icon::Warning,
        );

        modal.buttons(ui, |ui| {
            if modal.button(ui, "Cancel").clicked() {
                action = Some(AppAction::CloseDialog);
            }

            if modal.caution_button(ui, "Remove").clicked() {
                match self.modal {
                    ViewModal::RemovePromptHistory { idx, history_idx } => {
                        action = Some(AppAction::RemovePromptHistory { idx, history_idx });
                    }
                    _ => {
                        action = Some(AppAction::CloseDialog);
                    }
                }
            }
        });

        action
    }

    pub fn create_modify_prompt_modal(ctx: &egui::Context, id: String, width: f32) -> Modal {
        let style = ModalStyle {
            default_width: Some(width),
            ..Default::default()
        };

        Modal::new(ctx, id)
            .with_close_on_outside_click(true)
            .with_style(&style)
    }

    fn get_modify_prompt_modal_width(ctx: &egui::Context) -> f32 {
        ctx.available_rect().width() * 0.5
    }
}
