//! [`Prompt`] — a single-line text input dialog.

use crate::{ModalError, Theme};

/// Text input dialog.
///
/// ```no_run
/// use quick_modals::Prompt;
/// let name = Prompt::new("Your name?").placeholder("Jane").run().unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct Prompt {
    title: String,
    message: String,
    initial: String,
    placeholder: String,
    password: bool,
    confirm_label: String,
    theme: Theme,
    app_id: String,
}

impl Prompt {
    /// New dialog with a window title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: String::new(),
            initial: String::new(),
            placeholder: String::new(),
            password: false,
            confirm_label: "OK".to_string(),
            theme: Theme::default(),
            app_id: "quick-modals".to_string(),
        }
    }

    /// Body text shown above the field.
    pub fn message(mut self, msg: impl Into<String>) -> Self {
        self.message = msg.into();
        self
    }

    /// Pre-filled value.
    pub fn initial(mut self, v: impl Into<String>) -> Self {
        self.initial = v.into();
        self
    }

    /// Placeholder (hint) text.
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }

    /// Mask input (password style). Defaults to false.
    pub fn password(mut self, yes: bool) -> Self {
        self.password = yes;
        self
    }

    /// Confirm button text. Defaults to `"OK"`.
    pub fn confirm_label(mut self, s: impl Into<String>) -> Self {
        self.confirm_label = s.into();
        self
    }

    /// Visual theme override.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Wayland app_id (and X11 class hint).
    pub fn app_id(mut self, id: impl Into<String>) -> Self {
        self.app_id = id.into();
        self
    }

    /// Show the dialog and block. `Ok(None)` = Esc / window closed.
    pub fn run(self) -> Result<Option<String>, ModalError> {
        use std::sync::{Arc, Mutex};
        let shared: Arc<Mutex<Option<Option<String>>>> = Arc::new(Mutex::new(None));
        let inner = shared.clone();
        let title = self.title.clone();
        let theme = self.theme.clone();
        let app_id = self.app_id.clone();

        let viewport = egui::ViewportBuilder::default()
            .with_title(&title)
            .with_app_id(&app_id)
            .with_inner_size([theme.width.min(480.0), 190.0])
            .with_min_inner_size([320.0, 150.0])
            .with_decorations(false)
            .with_always_on_top()
            .with_resizable(false)
            .with_transparent(true);
        let message = self.message.clone();
        let placeholder = self.placeholder.clone();
        let initial = self.initial.clone();
        let password = self.password;
        let confirm_label = self.confirm_label.clone();
        let res = eframe::run_native(
            &title,
            eframe::NativeOptions {
                viewport,
                ..Default::default()
            },
            Box::new(move |_cc| {
                struct App {
                    result: Arc<Mutex<Option<Option<String>>>>,
                    value: String,
                    message: String,
                    placeholder: String,
                    password: bool,
                    confirm_label: String,
                    theme: Theme,
                    focused: bool,
                    id: egui::Id,
                }
                impl eframe::App for App {
                    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
                        [0.0, 0.0, 0.0, 0.0]
                    }
                    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
                        let esc = ctx
                            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                        let enter = ctx
                            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                        let mut done: Option<Option<String>> = None;
                        if esc {
                            done = Some(None);
                        }
                        if enter {
                            done = Some(Some(self.value.clone()));
                        }
                        let frame = egui::Frame::new()
                            .fill(self.theme.background)
                            .corner_radius(self.theme.corner_radius)
                            .inner_margin(self.theme.padding)
                            .outer_margin(self.theme.margin);
                        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                            if !self.message.is_empty() {
                                ui.label(
                                    egui::RichText::new(&self.message)
                                        .size(self.theme.font_size)
                                        .color(self.theme.text),
                                );
                                ui.add_space(8.0);
                            }
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut self.value)
                                    .id(self.id)
                                    .hint_text(&self.placeholder)
                                    .password(self.password)
                                    .desired_width(f32::INFINITY),
                            );
                            if !self.focused {
                                resp.request_focus();
                                self.focused = true;
                            }
                            ui.add_space(8.0);
                            if ui.button(&self.confirm_label).clicked() {
                                done = Some(Some(self.value.clone()));
                            }
                        });
                        if let Some(v) = done {
                            if let Ok(mut g) = self.result.lock() {
                                *g = Some(v);
                            }
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
                Ok(Box::new(App {
                    result: inner,
                    value: initial,
                    message,
                    placeholder,
                    password,
                    confirm_label,
                    theme,
                    focused: false,
                    id: egui::Id::new("quick-modals-prompt"),
                }) as Box<dyn eframe::App>)
            }),
        );
        res.map_err(|e| ModalError::Backend(e.to_string()))?;
        Ok(shared.lock().map(|g| g.clone().flatten()).unwrap_or(None))
    }
}
