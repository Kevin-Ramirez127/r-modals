//! [`Confirm`] — a yes/no dialog on the same borderless backend.

use crate::{ModalError, Theme};

/// One-shot confirmation dialog.
///
/// ```no_run
/// use quick_modals::Confirm;
/// let ok = Confirm::new("Delete file?").message("This cannot be undone.").run().unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct Confirm {
    title: String,
    message: String,
    confirm_label: String,
    cancel_label: String,
    theme: Theme,
    app_id: String,
}

impl Confirm {
    /// New dialog with a window title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: String::new(),
            confirm_label: "Confirm".to_string(),
            cancel_label: "Cancel".to_string(),
            theme: Theme::default(),
            app_id: "quick-modals".to_string(),
        }
    }

    /// Body text shown under the title.
    pub fn message(mut self, msg: impl Into<String>) -> Self {
        self.message = msg.into();
        self
    }

    /// Confirm button text. Defaults to `"Confirm"`.
    pub fn confirm_label(mut self, s: impl Into<String>) -> Self {
        self.confirm_label = s.into();
        self
    }

    /// Cancel button text. Defaults to `"Cancel"`.
    pub fn cancel_label(mut self, s: impl Into<String>) -> Self {
        self.cancel_label = s.into();
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

    /// Show the dialog and block. `Ok(true)` = confirmed, `Ok(false)` =
    /// cancelled / Esc / window closed.
    pub fn run(self) -> Result<bool, ModalError> {
        use std::sync::{Arc, Mutex};
        let shared: Arc<Mutex<Option<bool>>> = Arc::new(Mutex::new(None));
        let inner = shared.clone();
        let title = self.title.clone();
        let theme = self.theme.clone();
        let app_id = self.app_id.clone();
        let confirm_label = self.confirm_label.clone();
        let cancel_label = self.cancel_label.clone();
        let message = self.message.clone();

        let viewport = egui::ViewportBuilder::default()
            .with_title(&title)
            .with_app_id(&app_id)
            .with_inner_size([theme.width.min(420.0), 180.0])
            .with_min_inner_size([320.0, 150.0])
            .with_decorations(false)
            .with_always_on_top()
            .with_resizable(false)
            .with_transparent(true);
        let res = eframe::run_native(
            &title,
            eframe::NativeOptions {
                viewport,
                ..Default::default()
            },
            Box::new(move |_cc| {
                struct App {
                    result: Arc<Mutex<Option<bool>>>,
                    message: String,
                    confirm_label: String,
                    cancel_label: String,
                    theme: Theme,
                    focused: bool,
                }
                impl eframe::App for App {
                    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
                        let enter = ctx
                            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                        let esc = ctx
                            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                        let mut done: Option<bool> = None;
                        if enter {
                            done = Some(true);
                        }
                        if esc {
                            done = Some(false);
                        }
                        let frame = egui::Frame::new()
                            .fill(self.theme.background)
                            .corner_radius(self.theme.corner_radius)
                            .stroke(egui::Stroke::new(
                                self.theme.border_width,
                                self.theme.border,
                            ))
                            .inner_margin(self.theme.padding)
                            .outer_margin(self.theme.margin);
                        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                            if !self.message.is_empty() {
                                ui.label(
                                    egui::RichText::new(&self.message)
                                        .size(self.theme.font_size)
                                        .color(self.theme.text),
                                );
                                ui.add_space(12.0);
                            }
                            ui.horizontal(|ui| {
                                let c = ui.button(
                                    egui::RichText::new(&self.confirm_label)
                                        .size(self.theme.font_size)
                                        .color(self.theme.text),
                                );
                                if !self.focused {
                                    c.request_focus();
                                    self.focused = true;
                                }
                                if c.clicked() {
                                    done = Some(true);
                                }
                                if ui.button(&self.cancel_label).clicked() {
                                    done = Some(false);
                                }
                            });
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
                    message,
                    confirm_label,
                    cancel_label,
                    theme,
                    focused: false,
                }) as Box<dyn eframe::App>)
            }),
        );
        res.map_err(|e| ModalError::Backend(e.to_string()))?;
        Ok(shared.lock().map(|g| g.unwrap_or(false)).unwrap_or(false))
    }
}
