use crate::{filter_options_with, Layout, ModalError, ModalOption, Position, SearchConfig, Theme};
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
    /// True when the pick is free-typed text (no option matched and
    /// [`Modal::allow_custom_input`] was on). `id`/`label` equal the query.
    pub custom_input: bool,
}

/// Builder for an instant modal.
///
/// ```no_run
/// use r_modals::{Modal, ModalOption};
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
    pub(crate) default_selected: Option<String>,
    pub(crate) allow_custom_input: bool,
    pub(crate) empty_message: String,
    pub(crate) footer_hint: Option<String>,
    pub(crate) multi_select: bool,
    pub(crate) max_results: Option<usize>,
    pub(crate) fonts: Vec<(String, Vec<u8>)>,
    pub(crate) history_file: Option<String>,
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
            app_id: "r-modals".to_string(),
            default_selected: None,
            allow_custom_input: false,
            empty_message: "No matches".to_string(),
            footer_hint: None,
            multi_select: false,
            max_results: None,
            fonts: Vec::new(),
            history_file: None,
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

    /// Subsequence fuzzy match (ranked best-first) instead of substring.
    /// Defaults to `false`.
    pub fn fuzzy(mut self, yes: bool) -> Self {
        self.search.fuzzy = yes;
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
    /// use r_modals::Modal;
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
    /// use r_modals::Modal;
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
    pub(crate) fn resolved_theme(&self) -> Result<Theme, ModalError> {
        let mut theme = self.theme.clone();
        if let Some(css) = &self.stylesheet {
            theme.apply_css(css).map_err(ModalError::Css)?;
        }
        if let Some(css) = &self.stylesheet_override {
            theme.apply_css(css).map_err(ModalError::Css)?;
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
    /// Defaults to `"r-modals"`. Tiling compositors ignore requested
    /// window sizes unless the window floats — target this id in your
    /// compositor config, e.g. sway:
    /// `for_window [app_id="r-modals"] floating enable`.
    pub fn app_id(mut self, id: impl Into<String>) -> Self {
        self.app_id = id.into();
        self
    }

    /// Pre-select the option with this `id` (initial cursor position).
    /// No-op when the id is absent from the initial visible list.
    pub fn default_selected(mut self, id: impl Into<String>) -> Self {
        self.default_selected = Some(id.into());
        self
    }

    /// When true, pressing Enter with no matches returns the typed query
    /// itself (`id == label == query`, `custom_input: true`) instead of
    /// doing nothing. Command-palette style. Defaults to `false`.
    pub fn allow_custom_input(mut self, yes: bool) -> Self {
        self.allow_custom_input = yes;
        self
    }

    /// Message shown when the filter matches nothing. Defaults to
    /// `"No matches"`.
    pub fn empty_message(mut self, msg: impl Into<String>) -> Self {
        self.empty_message = msg.into();
        self
    }

    /// Small hint line rendered at the bottom of the modal
    /// (e.g. `"Enter pick · Esc cancel · Space toggle"`). Defaults to none.
    pub fn footer_hint(mut self, hint: impl Into<String>) -> Self {
        self.footer_hint = Some(hint.into());
        self
    }

    /// Space-toggles multi-select; Enter confirms. Prefer
    /// [`Modal::run_multi`] to collect every pick — [`Modal::run`] still
    /// returns only the first pick in this mode for compatibility.
    pub fn multi_select(mut self, yes: bool) -> Self {
        self.multi_select = yes;
        self
    }

    /// Cap the number of rows/tiles shown (best matches first in fuzzy
    /// mode). `None` (default) shows everything. Useful for 10k-option
    /// lists.
    pub fn max_results(mut self, n: usize) -> Self {
        self.max_results = Some(n);
        self
    }

    /// Register a custom font from TTF/OTF bytes. `name` becomes the
    /// egui font family name; CSS `font-size` still controls sizing.
    /// Installed into the egui context at startup; also set as default.
    pub fn font(mut self, name: impl Into<String>, ttf_bytes: Vec<u8>) -> Self {
        self.fonts.push((name.into(), ttf_bytes));
        self
    }

    /// Persist pick counts to this file (one `id count` per line) and
    /// sort top-level options most-used-first on open. Missing/unreadable
    /// files are ignored; failures to save are ignored.
    pub fn history_file(mut self, path: impl Into<String>) -> Self {
        self.history_file = Some(path.into());
        self
    }

    /// Filter without opening a window (useful for tests / headless use).
    pub fn filtered_indices(&self, query: &str) -> Vec<usize> {
        if !self.search.enabled {
            return Self::truncate((0..self.options.len()).collect(), self.max_results);
        }
        Self::truncate(
            filter_options_with(&self.options, query, &self.search),
            self.max_results,
        )
    }

    pub(crate) fn truncate(mut v: Vec<usize>, max: Option<usize>) -> Vec<usize> {
        if let Some(m) = max {
            v.truncate(m);
        }
        v
    }

    /// Load `id -> count` history. Best-effort: errors yield an empty map.
    pub(crate) fn load_history(path: &str) -> std::collections::HashMap<String, u64> {
        let mut map = std::collections::HashMap::new();
        let Ok(text) = std::fs::read_to_string(path) else {
            return map;
        };
        for line in text.lines() {
            let mut parts = line.rsplitn(2, ' ');
            let (count, id) = match (parts.next(), parts.next()) {
                (Some(c), Some(id)) => (c, id),
                _ => continue,
            };
            if let Ok(n) = count.parse::<u64>() {
                map.insert(id.to_string(), n);
            }
        }
        map
    }

    /// Sort top-level options most-used-first (stable: ties keep order).
    pub(crate) fn apply_history(&mut self) {
        let Some(path) = self.history_file.clone() else {
            return;
        };
        let counts = Self::load_history(&path);
        if counts.is_empty() {
            return;
        }
        self.options.sort_by(|a, b| {
            let ca = counts.get(&a.id).copied().unwrap_or(0);
            let cb = counts.get(&b.id).copied().unwrap_or(0);
            cb.cmp(&ca)
        });
    }

    pub(crate) fn record_history(&self, id: &str) {
        let Some(path) = &self.history_file else {
            return;
        };
        let mut counts = Self::load_history(path);
        *counts.entry(id.to_string()).or_insert(0) += 1;
        let mut rows: Vec<(&String, &u64)> = counts.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let text = rows
            .iter()
            .map(|(id, n)| format!("{id} {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let _ = std::fs::write(path, text);
    }

    /// Collect `(index-path, action, on_error)` for every option with an
    /// action, so [`Modal::run`] can fire the pick's action after the modal
    /// is gone (the modal itself is moved into the GUI closure).
    fn collect_actions(
        &self,
    ) -> Vec<(Vec<usize>, crate::SelectAction, Option<crate::ErrorAction>)> {
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
    pub fn run(self) -> Result<Option<ModalSelection>, ModalError> {
        if self.options.is_empty() {
            return Err(ModalError::NoOptions);
        }
        // Actions are cloned out first: `self` moves into the GUI closure.
        let actions = self.collect_actions();
        let mut modal = self;
        modal.apply_history();
        modal.theme = modal.resolved_theme()?;
        let history_file = modal.history_file.clone();
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
            return Err(ModalError::Backend(e.to_string()));
        }
        let picked: Option<ModalSelection> = shared.lock().map(|g| g.clone()).unwrap_or(None);
        if let Some(ref sel) = picked {
            if let Some(path) = &history_file {
                // Best-effort persistence; failures are ignored.
                let m = Modal::new("").history_file(path.clone());
                m.record_history(&sel.id);
            }
            Modal::dispatch_action(&actions, sel);
        }
        Ok(picked)
    }

    /// Multi-select variant: Space toggles rows, Enter confirms the set.
    /// Returns every pick (empty = cancelled / nothing toggled).
    /// Actions fire once per pick, in order, after the window closes.
    pub fn run_multi(self) -> Result<Vec<ModalSelection>, ModalError> {
        let multi = self.multi_select(true);
        crate::multi::run_multi(multi)
    }

    /// Best-effort non-blocking variant: runs [`Modal::run`] on a spawned
    /// thread and delivers the result to `callback`.
    ///
    /// Note: some platforms prefer GUI on the main thread; if the backend
    /// fails the callback receives the [`ModalError`].
    pub fn spawn(
        self,
        callback: impl FnOnce(Result<Option<ModalSelection>, ModalError>) + Send + 'static,
    ) {
        std::thread::spawn(move || callback(self.run()));
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
            .option(ModalOption::new("p", "Parent").sub_options([
                ModalOption::new("b", "B"),
                ModalOption::new("c", "C").shell("echo-c"),
            ]));
        let actions = m.collect_actions();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].0, vec![0]);
        assert!(matches!(actions[0].1, crate::SelectAction::Command { .. }));
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
            custom_input: false,
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
            custom_input: false,
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

    #[test]
    fn new_builder_defaults() {
        let m = Modal::new("x");
        assert!(m.default_selected.is_none());
        assert!(!m.allow_custom_input);
        assert!(!m.multi_select);
        assert!(m.max_results.is_none());
        assert!(m.footer_hint.is_none());
        assert!(m.fonts.is_empty());
        assert!(m.history_file.is_none());
        assert!(!m.search.fuzzy);
        assert_eq!(m.empty_message, "No matches");
    }

    #[test]
    fn error_converts_to_string_for_legacy_mains() {
        let e = crate::ModalError::NoOptions;
        let s: String = e.into();
        assert!(s.contains("no options"));
        assert_eq!(
            Modal::new("x").run().unwrap_err().to_string(),
            "r-modals: no options provided"
        );
    }

    #[test]
    fn max_results_truncates_filtered_indices() {
        let m = Modal::new("x")
            .options((0..10).map(|i| ModalOption::new(format!("id{i}"), format!("Item {i}"))))
            .max_results(3);
        assert_eq!(m.filtered_indices("Item").len(), 3);
    }

    #[test]
    fn history_sorts_most_used_first() {
        let dir = std::env::temp_dir().join(format!("qm-hist-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hist.txt");
        std::fs::write(&path, "b 5\na 1\n").unwrap();
        let mut m = Modal::new("x")
            .option(ModalOption::new("a", "A"))
            .option(ModalOption::new("b", "B"))
            .option(ModalOption::new("c", "C"))
            .history_file(path.to_str().unwrap());
        m.apply_history();
        assert_eq!(m.options[0].id, "b");
        assert_eq!(m.options[1].id, "a");
        assert_eq!(m.options[2].id, "c");
        m.record_history("c");
        let counts = Modal::load_history(path.to_str().unwrap());
        assert_eq!(counts.get("c"), Some(&1));
        assert_eq!(counts.get("b"), Some(&5));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
