use crate::{filter_options, Layout, ModalOption, Position, SearchConfig, Theme};
use std::sync::{Arc, Mutex};

/// What the user picked. `None` (from [`Modal::run`]) means Esc / window closed.
#[derive(Debug, Clone)]
pub struct ModalSelection {
    /// The `id` of the chosen option.
    pub id: String,
    /// The `label` of the chosen option.
    pub label: String,
    /// Index of the chosen option within its own level (i.e. among its
    /// siblings). Always equals `path.last()`.
    pub index: usize,
    /// Index path from the top-level option list to the chosen leaf, one
    /// entry per level. Flat modals always yield a single entry.
    pub path: Vec<usize>,
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
    /// Second stylesheet, applied after [`Modal::stylesheet`].
    /// Wins wherever both specify something.
    pub(crate) stylesheet_override: Option<String>,
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
            stylesheet_override: None,
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

    /// Inject a CSS snippet that overrides both the [`Theme`] and the
    /// [`Modal::stylesheet`] file. Layering order (later wins):
    /// theme → `stylesheet` → `stylesheet_override`.
    ///
    /// Useful when the base theme lives in a file and call-site tweaks go
    /// inline:
    ///
    /// ```no_run
    /// use quick_modals::Modal;
    /// let sel = Modal::new("Pick one")
    ///     .stylesheet("modal { background: #18181c; }")
    ///     .stylesheet_override("search { background: #222228; }")
    ///     .run();
    /// ```
    ///
    /// A bad override stylesheet makes [`Modal::run`] return `Err`, same as
    /// [`Modal::stylesheet`].
    pub fn stylesheet_override(mut self, css: impl Into<String>) -> Self {
        self.stylesheet_override = Some(css.into());
        self
    }

    /// Theme with the stylesheet (if any) applied. Used by [`Modal::run`].
    pub(crate) fn resolved_theme(&self) -> Result<Theme, String> {
        let mut theme = self.theme.clone();
        if let Some(css) = &self.stylesheet {
            theme.apply_css(css)?;
        }
        if let Some(css) = &self.stylesheet_override {
            theme.apply_css(css)?;
        }
        Ok(theme)
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

    /// Collect `(index-path, action, on_error)` for every option with an
    /// action, so [`Modal::run`] can fire the pick's action after the modal
    /// is gone (the modal itself is moved into the GUI closure).
    fn collect_actions(&self) -> Vec<(Vec<usize>, crate::SelectAction, Option<crate::ErrorAction>)> {
        fn walk(
            prefix: &mut Vec<usize>,
            opts: &[ModalOption],
            out: &mut Vec<(Vec<usize>, crate::SelectAction, Option<crate::ErrorAction>)>,
        ) {
            for (i, o) in opts.iter().enumerate() {
                if let Some(a) = &o.action {
                    prefix.push(i);
                    out.push((prefix.clone(), a.clone(), o.on_error.clone()));
                    prefix.pop();
                }
                if !o.children.is_empty() {
                    prefix.push(i);
                    walk(prefix, &o.children, out);
                    prefix.pop();
                }
            }
        }
        let mut out = Vec::new();
        walk(&mut Vec::new(), &self.options, &mut out);
        out
    }

    /// Fire the pick's action, or its `on_error` fallback when the spawn
    /// fails. Split out for tests (`run()` needs a display).
    pub(crate) fn dispatch_action(
        actions: &[(Vec<usize>, crate::SelectAction, Option<crate::ErrorAction>)],
        sel: &ModalSelection,
    ) {
        if let Some((_, action, on_error)) = actions.iter().find(|(p, _, _)| *p == sel.path) {
            if let Err(e) = action.execute(sel) {
                if let Some(f) = on_error {
                    f(sel, &e);
                }
            }
        }
    }

    /// Show the modal and block until the user picks or cancels.
    ///
    /// Returns `Ok(None)` on Esc / window close.
    /// Returns `Err` if the GUI backend fails to start (e.g. no display),
    /// or if the [`Modal::stylesheet`] fails to parse.
    ///
    /// If the picked option has an action ([`ModalOption::command`],
    /// [`ModalOption::shell`], [`ModalOption::on_select`]), it fires here —
    /// once, on the calling thread, after the window closes. A failing
    /// spawn fires the option's [`ModalOption::on_error`] fallback instead.
    pub fn run(self) -> Result<Option<ModalSelection>, String> {
        if self.options.is_empty() {
            return Err("quick-modals: no options provided".to_string());
        }
        // Actions are cloned out first: `self` moves into the GUI closure.
        let actions = self.collect_actions();
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
        let picked: Option<ModalSelection> =
            shared.lock().map(|g| g.clone()).unwrap_or(None);
        if let Some(ref sel) = picked {
            Modal::dispatch_action(&actions, sel);
        }
        Ok(picked)
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
    fn collect_actions_walks_nested_paths() {
        let m = Modal::new("x")
            .option(ModalOption::new("a", "A").command("echo-a"))
            .option(
                ModalOption::new("p", "Parent").sub_options([
                    ModalOption::new("b", "B"),
                    ModalOption::new("c", "C").shell("echo-c"),
                ]),
            );
        let actions = m.collect_actions();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].0, vec![0]);
        assert!(matches!(
            actions[0].1,
            crate::SelectAction::Command { .. }
        ));
        assert_eq!(actions[1].0, vec![1, 1]);
        assert!(matches!(actions[1].1, crate::SelectAction::Shell(_)));
    }

    #[test]
    fn dispatch_action_falls_back_on_spawn_failure() {
        use std::sync::{Arc, Mutex};
        let sel = ModalSelection {
            id: "bad".to_string(),
            label: "Bad".to_string(),
            index: 0,
            path: vec![0],
            query: String::new(),
        };
        // No panic without a handler either.
        let bare = Modal::new("y")
            .option(ModalOption::new("bad", "Bad").command("definitely-not-a-real-binary"));
        let bare_actions = bare.collect_actions();
        Modal::dispatch_action(&bare_actions, &sel);

        // With a handler: it fires exactly once, with the pick and the error.
        let fired = Arc::new(Mutex::new(Vec::new()));
        let fired2 = fired.clone();
        let m2 = Modal::new("x").option(
            ModalOption::new("bad", "Bad")
                .command("definitely-not-a-real-binary")
                .on_error(move |sel, err| {
                    fired2
                        .lock()
                        .unwrap()
                        .push((sel.id.clone(), err.to_string()));
                }),
        );
        Modal::dispatch_action(&m2.collect_actions(), &sel);
        let fired = fired.lock().unwrap();
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].0, "bad");
        assert!(!fired[0].1.is_empty());
    }

    #[test]
    fn dispatch_action_skips_fallback_on_success() {
        use std::sync::{Arc, Mutex};
        let fired = Arc::new(Mutex::new(false));
        let fired2 = fired.clone();
        // Callbacks cannot spawn-fail: fallback must stay quiet.
        let m = Modal::new("x").option(
            ModalOption::new("ok", "Ok")
                .on_select(|_| {})
                .on_error(move |_, _| *fired2.lock().unwrap() = true),
        );
        let sel = ModalSelection {
            id: "ok".to_string(),
            label: "Ok".to_string(),
            index: 0,
            path: vec![0],
            query: String::new(),
        };
        Modal::dispatch_action(&m.collect_actions(), &sel);
        assert!(!*fired.lock().unwrap());
    }

    #[test]
    fn stylesheet_override_wins_over_stylesheet_and_theme() {
        let m = Modal::new("x")
            .theme(Theme::from_css("modal { width: 100px; color: red; }").unwrap())
            .stylesheet("modal { width: 200px; color: blue; }")
            .stylesheet_override("modal { width: 300px; }");
        let t = m.resolved_theme().unwrap();
        // Override wins where specified…
        assert_eq!(t.width, 300.0);
        // …stylesheet wins where the override is silent…
        assert_eq!(t.text, egui::Color32::from_rgb(0, 0, 255));
        // …and a bad override errors like a bad stylesheet.
        assert!(Modal::new("x")
            .stylesheet_override("modal { width: 10em; }")
            .resolved_theme()
            .is_err());
        // No override: previous behavior unchanged.
        let t = Modal::new("x")
            .stylesheet("modal { width: 200px; }")
            .resolved_theme()
            .unwrap();
        assert_eq!(t.width, 200.0);
    }

    #[test]
    fn stylesheet_layers_over_theme_and_bad_css_errors() {
        let base = Theme::from_css("modal { width: 100px; }").unwrap();
        let m = Modal::new("x")
            .theme(base)
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
