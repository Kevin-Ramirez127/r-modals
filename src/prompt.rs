//! [`Prompt`] — a single-line text input dialog.

use crate::window::{run_app, WindowSpec};
use crate::{ModalError, Position, Theme};

/// Text input dialog.
///
/// ```no_run
/// use r_modals::Prompt;
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
    position: Position,
    stylesheet: Option<String>,
    show_title: bool,
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
            app_id: "r-modals".to_string(),
            position: Position::Center,
            stylesheet: None,
            show_title: true,
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

    /// Style the dialog from CSS (same subset as [`Theme::from_css`]).
    pub fn stylesheet(mut self, css: impl Into<String>) -> Self {
        self.stylesheet = Some(css.into());
        self
    }

    /// Window placement.
    pub fn position(mut self, pos: Position) -> Self {
        self.position = pos;
        self
    }

    /// Render the title inside the borderless window. Defaults to `true`.
    ///
    /// Decorations are off, so the OS title bar is never visible; this is the
    /// label the user actually reads.
    pub fn show_title(mut self, yes: bool) -> Self {
        self.show_title = yes;
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
        let mut theme = self.theme.clone();
        if let Some(css) = &self.stylesheet {
            theme.apply_css(css).map_err(ModalError::Css)?;
        }
        let shared: Arc<Mutex<Option<Option<String>>>> = Arc::new(Mutex::new(None));
        let inner = shared.clone();
        let title = self.title.clone();
        let message = self.message.clone();
        let placeholder = self.placeholder.clone();
        let initial = self.initial.clone();
        let password = self.password;
        let confirm_label = self.confirm_label.clone();
        let show_title = self.show_title;

        let spec = WindowSpec::dialog(
            title.clone(),
            &theme,
            self.app_id,
            480.0,
            190.0,
            self.position,
        );
        run_app(spec, move |_cc| App {
            result: inner,
            title,
            show_title,
            message,
            placeholder,
            password,
            confirm_label,
            theme,
            focused: false,
            id: egui::Id::new("r-modals-prompt"),
            value: initial,
        })?;
        Ok(shared.lock().map(|g| g.clone().flatten()).unwrap_or(None))
    }
}

struct App {
    result: std::sync::Arc<std::sync::Mutex<Option<Option<String>>>>,
    title: String,
    show_title: bool,
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
        let esc = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
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
            if self.show_title && !self.title.is_empty() {
                ui.label(
                    egui::RichText::new(&self.title)
                        .size(self.theme.font_size * 1.05)
                        .strong()
                        .color(self.theme.text),
                );
                ui.add_space(6.0);
            }
            if !self.message.is_empty() {
                ui.label(
                    egui::RichText::new(&self.message)
                        .size(self.theme.font_size)
                        .color(self.theme.text),
                );
                ui.add_space(8.0);
            }
            let font = egui::FontId::proportional(self.theme.effective_search_font_size());
            let resp = egui::Frame::new()
                .fill(self.theme.search_background)
                .corner_radius(self.theme.effective_search_radius())
                .inner_margin(egui::Margin::symmetric(10, 7))
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.value)
                            .id(self.id)
                            .hint_text(
                                egui::RichText::new(&self.placeholder)
                                    .color(self.theme.search_hint),
                            )
                            .hint_text_font(font.clone())
                            .font(font)
                            .text_color(self.theme.search_text)
                            .password(self.password)
                            .frame(false)
                            .background_color(egui::Color32::TRANSPARENT)
                            .desired_width(f32::INFINITY)
                            .margin(egui::Vec2::ZERO),
                    )
                })
                .inner;
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
