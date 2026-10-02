use crate::{filter_options, Layout, ModalOption, Position, SearchConfig, Theme};
use std::sync::{Arc, Mutex};

/// What the user picked. `None` (from [`Modal::run`]) means Esc / window closed.
#[derive(Debug, Clone)]
pub struct ModalSelection {
    /// The `id` of the chosen option.
    pub id: String,
    /// The `label` of the chosen option.
    pub label: String,
    /// Index into the original option list passed to the modal.
    pub index: usize,
    /// Final search query text.
    pub query: String,
}

/// Builder for an instant modal.
///
/// ```no_run
/// use quick_modals::{Modal, ModalOption};
/// let sel = Modal::new("Pick one")
///     .option(ModalOption::new("a", "Alpha"))
///     .option(ModalOption::new("b", "Beta"))
///     .run()
///     .unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct Modal {
    pub(crate) title: String,
    pub(crate) options: Vec<ModalOption>,
    pub(crate) search: SearchConfig,
    pub(crate) theme: Theme,
    pub(crate) layout: Layout,
    pub(crate) center_on_navigate: bool,
    pub(crate) fit_width_to_content: bool,
    pub(crate) stylesheet: Option<String>,
    pub(crate) position: Position,
    pub(crate) initial_query: String,
    pub(crate) app_id: String,
}

impl Modal {
    /// New modal with a window title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            options: Vec::new(),
            search: SearchConfig::default(),
            theme: Theme::default(),
            layout: Layout::default(),
            center_on_navigate: true,
            fit_width_to_content: false,
            stylesheet: None,
            position: Position::Center,
            initial_query: String::new(),
            app_id: "quick-modals".to_string(),
        }
    }

    /// Append one option.
    pub fn option(mut self, opt: ModalOption) -> Self {
        self.options.push(opt);
        self
    }

    /// Append many options.
    pub fn options(mut self, opts: impl IntoIterator<Item = ModalOption>) -> Self {
        self.options.extend(opts);
        self
    }

    /// Enable/disable the search field.
    pub fn search(mut self, enabled: bool) -> Self {
        self.search.enabled = enabled;
        self
    }

    /// Placeholder (hint) text for the search field.
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.search.placeholder = text.into();
        self
    }

    /// Case-sensitive search. Defaults to `false`.
    pub fn case_sensitive(mut self, yes: bool) -> Self {
        self.search.case_sensitive = yes;
        self
    }

    /// Full search config override.
    pub fn search_config(mut self, cfg: SearchConfig) -> Self {
        self.search = cfg;
        self
    }

    /// Pre-fill the search field.
    pub fn initial_query(mut self, q: impl Into<String>) -> Self {
        self.initial_query = q.into();
        self
    }

    /// Visual theme override.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// List rows (thumbnail + text) or grid squares (image only).
    /// Defaults to [`Layout::List`].
    pub fn layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// Center the selected row/tile in the scroll area on keyboard
    /// navigation. Defaults to `true`; pass `false` to leave the
    /// scroll position untouched.
    pub fn center_on_navigate(mut self, yes: bool) -> Self {
        self.center_on_navigate = yes;
        self
    }

    /// Shrink the window width to fit the content instead of using the
    /// full [`Theme`] width. The width never drops below 320px and never
    /// exceeds the theme width. Defaults to `false`.
    pub fn fit_width_to_content(mut self, yes: bool) -> Self {
        self.fit_width_to_content = yes;
        self
    }

    /// Style the modal from a CSS stylesheet (see [`Theme::from_css`] for
    /// the supported selectors and properties). The stylesheet is layered
    /// over [`Theme`] on top of the configured theme, so it wins where it
    /// specifies something and the theme fills in the rest. A bad
    /// stylesheet makes [`Modal::run`] return `Err`.
    ///
    /// ```no_run
    /// use quick_modals::Modal;
    /// let sel = Modal::new("Pick one")
    ///     .stylesheet("modal { background: #18181c; }")
    ///     .run();
    /// ```
    pub fn stylesheet(mut self, css: impl Into<String>) -> Self {
        self.stylesheet = Some(css.into());
        self
    }

    /// Theme with the stylesheet (if any) applied. Used by [`Modal::run`].
    pub(crate) fn resolved_theme(&self) -> Result<Theme, String> {
        let mut theme = self.theme.clone();
        if let Some(css) = &self.stylesheet {
            theme.apply_css(css)?;
        }
        Ok(theme)
    }

    /// Window width/height shortcut.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.theme.width = width;
        self.theme.height = height;
        self
    }

    /// Window placement.
    pub fn position(mut self, pos: Position) -> Self {
        self.position = pos;
        self
    }

    /// Wayland app_id (and X11 class hint) for the window.
    ///
    /// Defaults to `"quick-modals"`. Tiling compositors ignore requested
    /// window sizes unless the window floats — target this id in your
    /// compositor config, e.g. sway:
    /// `for_window [app_id="quick-modals"] floating enable`.
    pub fn app_id(mut self, id: impl Into<String>) -> Self {
        self.app_id = id.into();
        self
    }

    /// Filter without opening a window (useful for tests / headless use).
    pub fn filtered_indices(&self, query: &str) -> Vec<usize> {
        if !self.search.enabled {
            return (0..self.options.len()).collect();
        }
        filter_options(&self.options, query, self.search.case_sensitive)
    }

    /// Show the modal and block until the user picks or cancels.
    ///
    /// Returns `Ok(None)` on Esc / window close.
    /// Returns `Err` if the GUI backend fails to start (e.g. no display),
    /// or if the [`Modal::stylesheet`] fails to parse.
    pub fn run(self) -> Result<Option<ModalSelection>, String> {
        if self.options.is_empty() {
            return Err("quick-modals: no options provided".to_string());
        }
        let mut modal = self;
        modal.theme = modal.resolved_theme()?;
        let shared: Arc<Mutex<Option<ModalSelection>>> = Arc::new(Mutex::new(None));
        let inner = shared.clone();
        let title = modal.title.clone();
        let theme = modal.theme.clone();
        let position = modal.position;
        let app_id = modal.app_id.clone();

        let viewport = {
            let mut v = egui::ViewportBuilder::default()
                .with_title(&title)
                .with_app_id(&app_id)
                .with_inner_size([theme.width, theme.height])
                .with_min_inner_size([320.0, 200.0])
                .with_decorations(false)
                .with_always_on_top()
                .with_resizable(true)
                .with_transparent(true);
            match position {
                Position::Center => {}
                Position::TopCenter { y_offset } => {
                    // Best-effort: eframe centers by default; offset must be
                    // applied by the compositor/WM. We keep the variant for
                    let _ = y_offset;
                }
                Position::Custom { x, y } => {
                    v = v.with_position([x as f32, y as f32]);
                }
            }
            v
        };

        let options = eframe::NativeOptions {
            viewport,
            ..Default::default()
        };

        let modal = modal;
        let res = eframe::run_native(
            &title,
            options,
            Box::new(move |cc| {
                Ok(Box::new(crate::app::ModalApp::new(cc, modal, inner)) as Box<dyn eframe::App>)
            }),
        );

        if let Err(e) = res {
            return Err(format!("quick-modals backend error: {e}"));
        }
        Ok(shared.lock().map(|g| g.clone()).unwrap_or(None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_on_navigate_defaults_on_and_is_settable() {
        assert!(Modal::new("x").center_on_navigate);
        assert!(!Modal::new("x").center_on_navigate(false).center_on_navigate);
    }

    #[test]
    fn fit_width_defaults_off_and_is_settable() {
        assert!(!Modal::new("x").fit_width_to_content);
        assert!(
            Modal::new("x")
                .fit_width_to_content(true)
                .fit_width_to_content
        );
    }

    #[test]
    fn stylesheet_layers_over_theme_and_bad_css_errors() {
        let m = Modal::new("x")
            .theme(Theme::dark().width(100.0))
            .stylesheet("modal { width: 200px; background: #000000; }");
        let t = m.resolved_theme().unwrap();
        assert_eq!(t.width, 200.0);
        assert_eq!(t.background, egui::Color32::from_rgb(0, 0, 0));
        // Untouched fields keep the custom theme, not the dark default.
        assert_eq!(t.height, Theme::dark().height);
        assert!(Modal::new("x")
            .stylesheet("modal { width: 10px; color }")
            .resolved_theme()
            .is_err());
    }
}
