use crate::option::find_option_path;
use crate::prefix::{self, CompiledPrefix};
use crate::shortcut::ShortcutTarget;
use crate::{IconSource, Layout, Modal, ModalOption, ModalSelection};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Retry budget for fit-to-content resizes (frames). Covers the startup
/// race where the window manager drops the first `InnerSize` while the
/// window is still mapping, plus the frames the cascade needs to settle.
const MAX_FIT_SENDS: u32 = 60;

/// What activating a row/tile means: parents drill one level deeper,
/// leaves are picked.
pub(crate) enum RowAction {
    Drill { index: usize },
    Pick(ModalSelection),
}

pub(crate) struct ModalApp {
    modal: Modal,
    /// Options still arriving for [`Modal::run_streaming`].
    options_rx: Option<Receiver<Vec<ModalOption>>>,
    /// True until the first streaming options batch arrives.
    loading: bool,
    /// Shown while `loading` is true.
    loading_message: String,
    query: String,
    cursor: usize,
    textures: HashMap<String, egui::TextureHandle>,
    result: Arc<Mutex<Option<ModalSelection>>>,
    search_id: egui::Id,
    focused_once: bool,
    /// Keyboard moved the cursor: center that item on the next frame.
    scroll_to_cursor: bool,
    /// Last scroll offset of the option area (diagnostic, used by tests).
    last_scroll_offset: egui::Vec2,
    /// Width of the option area content, in logical pixels.
    last_content_width: f32,
    /// Width the content settled at (debounces transient multi-pass layouts).
    fit_candidate: f32,
    /// Consecutive updates the content width stayed put.
    fit_stable_frames: u32,
    /// Pointer position at the end of the last frame: hover only steals the
    /// cursor on fresh mouse movement, so it never fights keyboard nav.
    last_hover_pos: Option<egui::Pos2>,
    /// Width last sent via an `InnerSize` viewport command.
    last_sent_width: f32,
    /// Sends used for the current fit target. The first resize can be lost
    /// while the window is still mapping; without retries the send-once
    /// dedup stuck the modal wide (grid centered with side margins).
    fit_send_attempts: u32,
    /// Column count of the last rendered grid (drives row-wise Up/Down).
    last_columns: usize,
    /// Grid column count measured with the full (unfiltered) list. While a
    /// filter is active the layout reuses this instead of recomputing from
    /// the current available width — filtering narrows the measured content,
    /// which (with fit-to-content) narrows the window, which recomputes
    /// fewer columns and traps the restored full list in that narrow grid.
    remembered_columns: Option<usize>,
    /// Drill-down path: indices from the top-level option list to the
    /// options currently shown. Empty means the root level. Activating a
    /// parent pushes its index; Esc/Backspace/‹ pops back.
    nav_path: Vec<usize>,
    /// Precomputed texture cache keys, one per option *of the current
    /// level*. Computing the key for `Bytes` hashes the full image — doing
    /// that per row per frame would re-hash megabytes at 60fps, so keys are
    /// computed once per level and rebuilt on navigation (rare).
    level_icon_keys: Vec<Option<String>>,
    /// Lowercased `label + id + description` per option of the current
    /// level (including descendant text, so parents gateway-match), built
    /// once per level. The case-insensitive filter then scans these instead
    /// of allocating two Strings per option per keystroke.
    level_lowered: Vec<String>,
    /// Filter cache: recomputed only when query/config changes, not per frame
    /// (update() used to run the full filter 3x per frame).
    filter_query: String,
    filter_case_sensitive: bool,
    filter_fuzzy: bool,
    filter_enabled: bool,
    filter_max: Option<usize>,
    filter_min_match_chars: Option<usize>,
    filter_visible: Vec<usize>,
    filter_initialized: bool,
    /// egui style persists across frames: applying it once avoids cloning
    /// the full `Style` and invalidating visuals 60x/sec.
    style_applied: bool,
}

impl ModalApp {
    pub(crate) fn new(
        cc: &eframe::CreationContext<'_>,
        modal: Modal,
        result: Arc<Mutex<Option<ModalSelection>>>,
    ) -> Self {
        Self::new_with_stream(cc, modal, result, None, String::new())
    }

    pub(crate) fn new_with_stream(
        cc: &eframe::CreationContext<'_>,
        modal: Modal,
        result: Arc<Mutex<Option<ModalSelection>>>,
        options_rx: Option<Receiver<Vec<ModalOption>>>,
        loading_message: String,
    ) -> Self {
        let query = modal.initial_query.clone();
        let loading = options_rx.is_some() && modal.options.is_empty();
        let mut app = Self {
            modal,
            options_rx,
            loading,
            loading_message: if loading_message.is_empty() {
                "Loading…".to_string()
            } else {
                loading_message
            },
            query,
            cursor: 0,
            textures: HashMap::new(),
            result,
            search_id: egui::Id::new("r-modals-search"),
            focused_once: false,
            scroll_to_cursor: false,
            last_scroll_offset: egui::Vec2::ZERO,
            last_content_width: 0.0,
            fit_candidate: -1.0,
            fit_stable_frames: 0,
            last_hover_pos: None,
            last_sent_width: -1.0,
            fit_send_attempts: 0,
            last_columns: 1,
            remembered_columns: None,
            nav_path: Vec::new(),
            level_icon_keys: Vec::new(),
            level_lowered: Vec::new(),
            filter_query: String::new(),
            filter_case_sensitive: false,
            filter_fuzzy: false,
            filter_enabled: true,
            filter_max: None,
            filter_min_match_chars: None,
            filter_visible: Vec::new(),
            filter_initialized: false,
            style_applied: false,
        };
        // One-time per-option work so per-frame rendering never hashes image
        // bytes or (re)builds search strings.
        app.refresh_level_caches();
        app.refresh_filter_cache();
        // Custom fonts: install into egui and set as default family.
        if !app.modal.fonts.is_empty() {
            let mut fonts = egui::FontDefinitions::default();
            for (name, bytes) in &app.modal.fonts {
                fonts.font_data.insert(
                    name.clone(),
                    egui::FontData::from_owned(bytes.clone()).into(),
                );
                if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                    fam.insert(0, name.clone());
                }
                if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                    fam.insert(0, name.clone());
                }
            }
            cc.egui_ctx.set_fonts(fonts);
        }
        // Initial cursor honors `default_selected` when visible.
        app.select_default();
        app
    }

    /// Move the cursor to `default_selected`, if it is visible.
    fn select_default(&mut self) {
        let Some(want) = self.modal.default_selected.clone() else {
            return;
        };
        if let Some(pos) = self
            .filter_visible
            .iter()
            .position(|&i| self.level_slice().get(i).is_some_and(|o| o.id == want))
        {
            self.cursor = pos;
        }
    }

    /// Fold in options that arrived for [`crate::Modal::run_streaming`].
    ///
    /// Returns `true` when the producer went away before sending anything and
    /// the window was closed as cancelled.
    fn poll_options(&mut self, ctx: &egui::Context) -> bool {
        let Some(rx) = self.options_rx.take() else {
            return false;
        };
        let mut latest = None;
        loop {
            match rx.try_recv() {
                Ok(options) => latest = Some(options),
                Err(TryRecvError::Empty) => {
                    self.options_rx = Some(rx);
                    break;
                }
                Err(TryRecvError::Disconnected) => break,
            }
        }
        if let Some(options) = latest {
            self.modal.options = options;
            self.loading = false;
            self.nav_path.clear();
            self.cursor = 0;
            self.remembered_columns = None;
            self.refresh_level_caches();
            self.filter_initialized = false;
            self.refresh_filter_cache();
            self.select_default();
        }
        if self.loading && self.options_rx.is_none() {
            self.loading = false;
            self.finish(None, ctx);
            return true;
        }
        false
    }

    /// Apply the theme-derived egui style once; egui keeps it across frames.
    fn apply_style_once(&mut self, ctx: &egui::Context, t: &crate::Theme) {
        if self.style_applied {
            return;
        }
        let mut style = (*ctx.style()).clone();
        style.visuals.window_fill = t.background;
        style.visuals.panel_fill = t.background;
        style.visuals.selection.bg_fill = t.selected;
        style.visuals.widgets.inactive.bg_fill = t.surface;
        style.visuals.widgets.hovered.bg_fill = t.selected.linear_multiply(0.85);
        style.visuals.widgets.active.bg_fill = t.selected;
        ctx.set_style(style);
        self.style_applied = true;
    }

    /// The first frame(s) of [`crate::Modal::run_streaming`]: a themed window
    /// with a message instead of the picker. Only Esc is meaningful.
    fn show_loading(&self, ctx: &egui::Context, t: &crate::Theme) {
        let frame = egui::Frame::new()
            .fill(t.background)
            .corner_radius(t.corner_radius)
            .inner_margin(t.padding)
            .outer_margin(t.margin);
        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            ui.style_mut().override_text_style = Some(egui::TextStyle::Body);
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.38).max(8.0));
                ui.add(
                    egui::Spinner::new()
                        .size(t.font_size * 1.6)
                        .color(t.dim_text),
                );
                ui.add_space(10.0);
                ui.colored_label(t.dim_text, &self.loading_message);
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Esc to cancel")
                        .size(t.font_size * 0.78)
                        .color(t.dim_text),
                );
            });
        });
        // Keep polling the channel: the producer cannot wake egui itself.
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    /// Options of the currently shown level (root when `nav_path` is empty).
    fn level_slice(&self) -> &[ModalOption] {
        let mut level: &[ModalOption] = &self.modal.options;
        for &i in &self.nav_path {
            match level.get(i) {
                Some(opt) => level = &opt.children,
                None => return &[],
            }
        }
        level
    }

    /// Number of options in the current level.
    fn level_len(&self) -> usize {
        self.level_slice().len()
    }

    /// One option of the current level by sibling index.
    fn level_opt(&self, idx: usize) -> Option<&ModalOption> {
        self.level_slice().get(idx)
    }

    /// Labels from the root to the current level (breadcrumb).
    fn trail_labels(&self) -> Vec<String> {
        let mut labels = Vec::with_capacity(self.nav_path.len());
        let mut level: &[ModalOption] = &self.modal.options;
        for &i in &self.nav_path {
            match level.get(i) {
                Some(opt) => {
                    labels.push(opt.label.clone());
                    level = &opt.children;
                }
                None => break,
            }
        }
        labels
    }

    /// Rebuild the per-level caches after navigation (rare: user action, not
    /// per frame, so hashing image bytes here is fine).
    fn refresh_level_caches(&mut self) {
        let level = self.level_slice();
        let keys: Vec<Option<String>> = level.iter().map(|o| o.icon.cache_key(&o.id)).collect();
        // Recursive lowering: a parent gateway-matches descendant queries.
        let lowered: Vec<String> = level.iter().map(|o| o.haystack_lower()).collect();
        self.level_icon_keys = keys;
        self.level_lowered = lowered;
    }

    /// Shared reset after every navigation change.
    fn after_nav_change(&mut self) {
        self.cursor = 0;
        self.query.clear();
        self.remembered_columns = None;
        self.refresh_level_caches();
        self.filter_initialized = false;
    }

    /// Drill into a submenu. No-op unless `idx` is a parent in the current
    /// level (leaves are picked, not drilled).
    fn enter_level(&mut self, idx: usize) {
        if !matches!(self.level_opt(idx), Some(o) if o.has_children()) {
            return;
        }
        self.nav_path.push(idx);
        self.after_nav_change();
    }

    /// Back up one level. Returns false at the root (caller then cancels).
    fn exit_level(&mut self) -> bool {
        if self.nav_path.pop().is_none() {
            return false;
        }
        self.after_nav_change();
        true
    }

    /// Jump to an ancestor level shown in the breadcrumb (`0` = root).
    fn nav_to_depth(&mut self, depth: usize) {
        if depth < self.nav_path.len() {
            self.nav_path.truncate(depth);
            self.after_nav_change();
        }
    }

    /// Recompute the visible list only when the query or search config
    /// changed since the last frame. All callers in `update()` share the
    /// cached `filter_visible` via a single clone per frame.
    fn refresh_filter_cache(&mut self) {
        let enabled = self.modal.search.enabled;
        let case_sensitive = self.modal.search.case_sensitive;
        let fuzzy = self.modal.search.fuzzy;
        let max = self.modal.max_results;
        let min_match_chars = self.modal.search.min_match_chars;
        if self.filter_initialized
            && self.filter_query == self.query
            && self.filter_case_sensitive == case_sensitive
            && self.filter_fuzzy == fuzzy
            && self.filter_enabled == enabled
            && self.filter_max == max
            && self.filter_min_match_chars == min_match_chars
        {
            return;
        }
        // Empty query (or disabled search) lists everything, including
        // `unsearchable` rows — same contract as `filter_options`.
        // (`query` is cloned first so the scan below holds no borrow of
        // `self` while writing `filter_visible`.)
        let query = self.query.clone();
        // Navigate to the current level with shared borrows only; the
        // per-level lowered cache is index-aligned with it.
        let mut level: &[ModalOption] = &self.modal.options;
        for &i in &self.nav_path {
            match level.get(i) {
                Some(opt) => level = &opt.children,
                None => {
                    level = &[];
                    break;
                }
            }
        }
        let lowered = &self.level_lowered;
        // Hidden default view: short (or empty) queries reveal only exact
        // label/id matches; from `min` characters on, the normal rules apply.
        let hidden_short =
            enabled && min_match_chars.is_some_and(|m| query.trim().chars().count() < m);
        let mut visible: Vec<usize> = if hidden_short {
            crate::search::exact_match_indices(level, &query, case_sensitive)
        } else if !enabled || query.is_empty() {
            (0..level.len()).collect()
        } else if fuzzy {
            let mut scored: Vec<(usize, i32)> = level
                .iter()
                .enumerate()
                .filter_map(|(i, o)| o.fuzzy_score(&query, case_sensitive).map(|s| (i, s)))
                .collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            scored.into_iter().map(|(i, _)| i).collect()
        } else if case_sensitive {
            level
                .iter()
                .enumerate()
                .filter(|(_, o)| o.matches_query(&query, "", true))
                .map(|(i, _)| i)
                .collect()
        } else {
            let q = query.to_lowercase();
            level
                .iter()
                .enumerate()
                .filter(|(i, _)| lowered.get(*i).is_some_and(|h| h.contains(&q)))
                .map(|(i, _)| i)
                .collect()
        };
        if let Some(m) = max {
            visible.truncate(m);
        }
        self.filter_visible = visible;
        self.filter_query = self.query.clone();
        self.filter_case_sensitive = case_sensitive;
        self.filter_fuzzy = fuzzy;
        self.filter_enabled = enabled;
        self.filter_max = max;
        self.filter_min_match_chars = min_match_chars;
        self.filter_initialized = true;
    }

    fn visible_indices(&mut self) -> Vec<usize> {
        self.refresh_filter_cache();
        self.filter_visible.clone()
    }

    /// Label of the row under the cursor, for Tab completion. Empty queries
    /// and an already-complete label produce no completion, so Tab keeps its
    /// prefix-completion meaning and never clears an untouched field.
    fn focused_item_label(&mut self) -> Option<String> {
        if self.query.trim().is_empty() {
            return None;
        }
        let visible = self.visible_indices();
        let &idx = visible.get(self.cursor)?;
        let label = self.level_opt(idx)?.label.clone();
        (!label.is_empty() && label != self.query).then_some(label)
    }

    /// Whether the empty-state message applies. An untouched (empty or
    /// whitespace-only) search field must stay blank.
    fn shows_empty_message(&self) -> bool {
        !self.query.trim().is_empty()
    }

    fn activate(&self, visible: &[usize], pos: usize) -> Option<RowAction> {
        let &idx = visible.get(pos)?;
        let opt = self.level_opt(idx)?;
        if !opt.activatable() {
            return None;
        }
        let mut path = self.nav_path.clone();
        path.push(idx);
        if opt.has_children() {
            return Some(RowAction::Drill { index: idx });
        }
        Some(RowAction::Pick(ModalSelection {
            id: opt.id.clone(),
            label: opt.label.clone(),
            index: idx,
            path,
            query: self.query.clone(),
            custom_input: false,
            shortcut: None,
        }))
    }

    /// Activate the option at `path`: drill into a submenu, or pick a leaf.
    /// Used by file-configured shortcuts that target an option id.
    fn activate_path(&mut self, path: &[usize], ctx: &egui::Context) {
        let Some((&last, prefix)) = path.split_last() else {
            return;
        };
        // Move to the level that owns the target.
        self.nav_path = prefix.to_vec();
        self.after_nav_change();
        let info = self.level_opt(last).map(|o| {
            (
                o.has_children(),
                o.activatable(),
                o.id.clone(),
                o.label.clone(),
            )
        });
        match info {
            Some((true, _, _, _)) => self.enter_level(last),
            Some((false, true, id, label)) => self.finish(
                Some(ModalSelection {
                    id,
                    label,
                    index: last,
                    path: path.to_vec(),
                    query: self.query.clone(),
                    custom_input: false,
                    shortcut: None,
                }),
                ctx,
            ),
            _ => {}
        }
    }

    /// Fire a watched prefix's action with the query remainder, returning the
    /// selection handed to the action (the modal then closes with no pick).
    /// Split out so it is testable without a display.
    fn run_prefix(cp: &CompiledPrefix, rest: &str, nav_path: &[usize]) -> ModalSelection {
        let sel = ModalSelection {
            id: cp.prefix.clone(),
            label: cp.hint.clone(),
            index: 0,
            path: nav_path.to_vec(),
            query: rest.to_string(),
            custom_input: true,
            shortcut: None,
        };
        let _ = cp.execute(&sel, rest);
        sel
    }

    /// Consume a bound shortcut key, if one was pressed this frame, and act on
    /// it. Returns true when a shortcut fired (the caller then stops handling
    /// the frame's other keys).
    fn fire_shortcut(&mut self, ctx: &egui::Context) -> bool {
        if self.modal.compiled_shortcuts.is_empty() {
            return false;
        }
        // One input lock for the whole keymap; `consume_key` matches logically
        // and `compiled_shortcuts` is ordered most-specific-first, so
        // `Ctrl+Shift+S` is seen before `Ctrl+S`.
        let mut pressed: Option<usize> = None;
        {
            let shortcuts = &self.modal.compiled_shortcuts;
            ctx.input_mut(|i| {
                for (idx, sc) in shortcuts.iter().enumerate() {
                    if i.consume_key(sc.modifiers, sc.key) {
                        pressed = Some(idx);
                        break;
                    }
                }
            });
        }
        let Some(idx) = pressed else {
            return false;
        };
        match self.modal.compiled_shortcuts[idx].target.clone() {
            ShortcutTarget::Option(id) => {
                if let Some(path) = find_option_path(&self.modal.options, &id) {
                    self.activate_path(&path, ctx);
                }
            }
            ShortcutTarget::Pick(tag) => {
                // Pick the highlighted row like Enter would, but tag the
                // selection so the caller can interpret it differently.
                let visible = self.visible_indices();
                if visible.is_empty() {
                    return true;
                }
                let cursor = self.cursor.min(visible.len() - 1);
                match self.activate(&visible, cursor) {
                    Some(RowAction::Pick(mut sel)) => {
                        sel.shortcut = Some(tag);
                        self.finish(Some(sel), ctx);
                    }
                    Some(RowAction::Drill { index }) => self.enter_level(index),
                    None => {}
                }
            }
            ShortcutTarget::Action(action) => {
                // Standalone commands have no row to land on: run the action
                // (best-effort, detached) and close with no pick.
                let sel = ModalSelection {
                    id: "shortcut".to_string(),
                    label: self.modal.compiled_shortcuts[idx].hint.clone(),
                    index: 0,
                    path: Vec::new(),
                    query: String::new(),
                    custom_input: false,
                    shortcut: None,
                };
                let _ = action.execute(&sel);
                self.finish(None, ctx);
            }
        }
        true
    }

    fn finish(&self, sel: Option<ModalSelection>, ctx: &egui::Context) {
        if let Ok(mut g) = self.result.lock() {
            *g = sel;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn icon_texture(
        textures: &mut HashMap<String, egui::TextureHandle>,
        ctx: &egui::Context,
        key: Option<&String>,
        icon: &IconSource,
    ) -> Option<egui::TextureHandle> {
        let key = key?;
        if let Some(t) = textures.get(key) {
            return Some(t.clone());
        }
        let img = load_icon(icon)?;
        let tex = ctx.load_texture(key, img, egui::TextureOptions::LINEAR);
        textures.insert(key.clone(), tex.clone());
        Some(tex)
    }

    /// Label with the first query occurrence underlined in accent.
    /// Underline never changes glyph metrics, so highlighting can't shift
    /// rows (unlike bold). Falls back to a plain label on no match.
    fn highlighted_label(
        ui: &mut egui::Ui,
        label: &str,
        query: &str,
        case_sensitive: bool,
        size: f32,
        base: egui::Color32,
        accent: egui::Color32,
    ) {
        let range = if query.is_empty() {
            None
        } else if case_sensitive {
            label.find(query).map(|s| (s, s + query.len()))
        } else {
            let l = label.to_lowercase();
            let q = query.to_lowercase();
            l.find(&q).map(|s| {
                // Map byte offsets through char boundaries: lowercase can
                // shift widths, so resolve via char indices.
                let start_char = l[..s].chars().count();
                let q_chars = q.chars().count();
                let start = label
                    .char_indices()
                    .nth(start_char)
                    .map(|(b, _)| b)
                    .unwrap_or(label.len());
                let end = label
                    .char_indices()
                    .nth(start_char + q_chars)
                    .map(|(b, _)| b)
                    .unwrap_or(label.len());
                (start, end)
            })
        };
        match range {
            Some((s, e)) if s < e && e <= label.len() => {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    if s > 0 {
                        ui.label(egui::RichText::new(&label[..s]).size(size).color(base));
                    }
                    ui.label(
                        egui::RichText::new(&label[s..e])
                            .size(size)
                            .color(accent)
                            .underline(),
                    );
                    if e < label.len() {
                        ui.label(egui::RichText::new(&label[e..]).size(size).color(base));
                    }
                });
            }
            _ => {
                ui.label(egui::RichText::new(label).size(size).color(base));
            }
        }
    }

    /// List rows: thumbnail followed by label + description.
    fn show_list(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        visible: &[usize],
        t: &crate::Theme,
        hover_fresh: bool,
    ) {
        // Disjoint field borrows: rendering needs `textures` mutably while
        // reading the current level — borrowing whole `self` for the
        // option would clone every icon's `Vec<u8>` per row per frame, so
        // we navigate to a shared slice and never hold an owned option.
        let modal = &self.modal;
        let mut level: &[ModalOption] = &modal.options;
        for &i in &self.nav_path {
            match level.get(i) {
                Some(opt) => level = &opt.children,
                None => {
                    level = &[];
                    break;
                }
            }
        }
        let textures = &mut self.textures;
        let icon_keys = &self.level_icon_keys;
        let cursor = &mut self.cursor;
        let scroll_to_cursor = self.scroll_to_cursor;
        let query = self.query.clone();
        // Owned copy for building selection paths inside the layout closure.
        let nav_snapshot = self.nav_path.clone();
        let tile_radius = t.effective_tile_radius();
        let mut action: Option<RowAction> = None;
        let out = egui::ScrollArea::vertical()
            .max_height(t.height - 110.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = t.item_spacing;
                for (pos, &opt_idx) in visible.iter().enumerate() {
                    let opt = &level[opt_idx];
                    let selected = pos == *cursor;
                    let row_fill = if selected { t.selected } else { t.surface };

                    let row = egui::Frame::new()
                        .fill(row_fill)
                        .corner_radius(tile_radius)
                        .inner_margin(egui::Margin::symmetric(8, 6))
                        .show(ui, |ui| {
                            // Stretch rows full-width, unless fitting the
                            // window to the content (then rows size to text).
                            if !modal.fit_width_to_content {
                                ui.set_width(ui.available_width());
                            }
                            ui.horizontal(|ui| {
                                // Icon (cached texture or placeholder box).
                                let tex = Self::icon_texture(
                                    textures,
                                    ctx,
                                    icon_keys.get(opt_idx).and_then(|k| k.as_ref()),
                                    &opt.icon,
                                );
                                if let Some(tex) = tex {
                                    ui.add(
                                        egui::Image::new(&tex)
                                            .max_size(egui::Vec2::splat(t.icon_size))
                                            .corner_radius(4.0),
                                    );
                                } else if !matches!(opt.icon, IconSource::None) {
                                    // Broken image path: small placeholder so rows align.
                                    let (r, _) = ui.allocate_exact_size(
                                        egui::Vec2::splat(t.icon_size),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(
                                        r,
                                        4.0,
                                        t.background.linear_multiply(1.4),
                                    );
                                }
                                ui.vertical(|ui| {
                                    // NOTE: selected must not change the text
                                    // style (e.g. `.strong()`): bold text
                                    // measures wider/taller, so selecting a
                                    // row would resize it and shift every row
                                    // below it. Selection is shown by row fill.
                                    // Disabled rows render dimmed; separators
                                    // render as dim centered headers.
                                    let label_color =
                                        if opt.disabled { t.dim_text } else { t.text };
                                    if opt.separator {
                                        ui.label(
                                            egui::RichText::new(&opt.label)
                                                .size(t.font_size * 0.85)
                                                .color(t.dim_text),
                                        );
                                    } else {
                                        Self::highlighted_label(
                                            ui,
                                            &opt.label,
                                            &self.query,
                                            self.modal.search.case_sensitive,
                                            t.font_size,
                                            label_color,
                                            t.accent,
                                        );
                                    }
                                    if t.show_descriptions {
                                        if let Some(d) = &opt.description {
                                            ui.label(
                                                egui::RichText::new(d)
                                                    .size(t.font_size * 0.82)
                                                    .color(t.dim_text),
                                            );
                                        }
                                    }
                                });
                                // Right-aligned trailing hints: shortcut first,
                                // then submenu affordance.
                                if opt.shortcut.is_some() || opt.has_children() {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if opt.has_children() {
                                                ui.label(
                                                    egui::RichText::new("›")
                                                        .size(t.font_size)
                                                        .color(t.dim_text),
                                                );
                                            }
                                            if let Some(sc) = &opt.shortcut {
                                                ui.label(
                                                    egui::RichText::new(sc)
                                                        .size(t.font_size * 0.8)
                                                        .color(t.dim_text),
                                                );
                                            }
                                        },
                                    );
                                }
                            });
                        });

                    // Keyboard navigation: center the selected row.
                    if scroll_to_cursor && pos == *cursor {
                        row.response.scroll_to_me(Some(egui::Align::Center));
                    }
                    // Click + hover select (hover only on fresh mouse
                    // movement, so it never fights keyboard navigation).
                    if row.response.hovered() && hover_fresh {
                        *cursor = pos;
                    }
                    if row.response.clicked() {
                        let mut path = nav_snapshot.clone();
                        path.push(opt_idx);
                        // Disabled / separator rows never activate.
                        if !opt.activatable() {
                        } else if opt.has_children() {
                            action = Some(RowAction::Drill { index: opt_idx });
                        } else {
                            action = Some(RowAction::Pick(ModalSelection {
                                id: opt.id.clone(),
                                label: opt.label.clone(),
                                index: opt_idx,
                                path,
                                query: query.clone(),
                                custom_input: false,
                                shortcut: None,
                            }));
                        }
                        if action.is_some() {
                            break;
                        }
                    }
                }
            });
        self.last_scroll_offset = out.state.offset;
        self.last_content_width = out.content_size.x;
        match action {
            Some(RowAction::Drill { index }) => self.enter_level(index),
            Some(RowAction::Pick(sel)) => self.finish(Some(sel), ctx),
            None => {}
        }
    }

    /// Grid squares: each tile IS the image (no text, tooltip shows the label).
    fn show_grid(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        visible: &[usize],
        t: &crate::Theme,
        hover_fresh: bool,
    ) {
        let tile = t.tile_size;
        let inner = (tile - 8.0).max(16.0);
        let tile_radius = t.effective_tile_radius();
        // `cols` is written during layout but read by keyboard nav next frame.
        // `full_list` decides whether the measurement is worth remembering:
        // only the unfiltered grid reflects the real window width. Both are
        // relative to the current drill-down level, not the top-level list.
        let modal = &self.modal;
        let mut level: &[ModalOption] = &modal.options;
        for &i in &self.nav_path {
            match level.get(i) {
                Some(opt) => level = &opt.children,
                None => {
                    level = &[];
                    break;
                }
            }
        }
        let full_list = visible.len() == level.len();
        let remembered = self.remembered_columns;
        let mut cols_out = 1usize;
        let textures = &mut self.textures;
        let icon_keys = &self.level_icon_keys;
        let cursor = &mut self.cursor;
        let scroll_to_cursor = self.scroll_to_cursor;
        let query = self.query.clone();
        // Owned copy for building selection paths inside the layout closure.
        let nav_snapshot = self.nav_path.clone();
        let mut action: Option<RowAction> = None;
        let out = egui::ScrollArea::vertical()
            .max_height(t.height - 110.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                // Explicit columns from available width. NOTE: `horizontal_wrapped`
                // does not wrap `Frame` tiles (they size unbounded and run off the
                // right edge, clipped and unreachable), so a Grid is required.
                // While filtered, reuse the remembered full-list count: the
                // filtered content is narrower, and recomputing from it would
                // collapse the grid (e.g. to 2 columns) and stick there after
                // the filter is cleared.
                let outer = tile + 8.0; // tile + Frame inner_margin on both sides
                let computed = ((ui.available_width() + t.item_spacing) / (outer + t.item_spacing))
                    .floor() as usize;
                let computed = computed.max(1);
                let cols = if full_list {
                    computed
                } else {
                    remembered.unwrap_or(computed)
                };
                cols_out = cols;
                // Center the whole grid block in the modal: a bare Grid
                // left-aligns, leaving the slack on the right.
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    egui::Grid::new("r-modals-tiles")
                        .num_columns(cols)
                        .spacing([t.item_spacing, t.item_spacing])
                        .show(ui, |ui| {
                            for (pos, &opt_idx) in visible.iter().enumerate() {
                                let opt = &level[opt_idx];
                                let selected = pos == *cursor;
                                let tile_resp = egui::Frame::new()
                                    .fill(if selected { t.selected } else { t.surface })
                                    .corner_radius(tile_radius)
                                    // NOTE: selection is fill-only (modals are
                                    // always borderless): the selected tile
                                    // reads via its `selected` background.
                                    .inner_margin(4.0)
                                    .show(ui, |ui| {
                                        // Fixed tile footprint with the image
                                        // centered inside: a top-down Ui would
                                        // stick a smaller image to the top-left.
                                        let (tile_rect, _) = ui.allocate_exact_size(
                                            egui::Vec2::splat(tile),
                                            egui::Sense::hover(),
                                        );
                                        let img_rect = egui::Rect::from_center_size(
                                            tile_rect.center(),
                                            egui::Vec2::splat(inner),
                                        );
                                        if let Some(tex) = Self::icon_texture(
                                            textures,
                                            ctx,
                                            icon_keys.get(opt_idx).and_then(|k| k.as_ref()),
                                            &opt.icon,
                                        ) {
                                            ui.put(
                                                img_rect,
                                                egui::Image::new(&tex)
                                                    .fit_to_exact_size(egui::Vec2::splat(inner))
                                                    .corner_radius(4.0),
                                            );
                                        } else {
                                            // Missing/broken image: placeholder square.
                                            ui.painter().rect_filled(
                                                img_rect,
                                                4.0,
                                                t.background.linear_multiply(1.4),
                                            );
                                        }
                                    })
                                    .response;
                                // Submenu affordance: parents drill deeper (›).
                                let tile_resp = if opt.has_children() {
                                    tile_resp.on_hover_text(format!("{} ›", opt.label))
                                } else {
                                    tile_resp.on_hover_text(&opt.label)
                                };
                                // Keyboard navigation: center the selected tile.
                                if scroll_to_cursor && pos == *cursor {
                                    tile_resp.scroll_to_me(Some(egui::Align::Center));
                                }
                                // Hover takes the cursor only on fresh mouse movement,
                                // so it never fights keyboard navigation.
                                if tile_resp.hovered() && hover_fresh {
                                    *cursor = pos;
                                }
                                if tile_resp.clicked() && opt.activatable() {
                                    let mut path = nav_snapshot.clone();
                                    path.push(opt_idx);
                                    action = if opt.has_children() {
                                        Some(RowAction::Drill { index: opt_idx })
                                    } else {
                                        Some(RowAction::Pick(ModalSelection {
                                            id: opt.id.clone(),
                                            label: opt.label.clone(),
                                            index: opt_idx,
                                            path,
                                            query: query.clone(),
                                            custom_input: false,
                                            shortcut: None,
                                        }))
                                    };
                                    break;
                                }
                                if (pos + 1) % cols == 0 || pos + 1 == visible.len() {
                                    ui.end_row();
                                }
                            }
                        });
                });
            });
        self.last_columns = cols_out;
        if full_list {
            self.remembered_columns = Some(cols_out);
        }
        self.last_scroll_offset = out.state.offset;
        self.last_content_width = out.content_size.x;
        match action {
            Some(RowAction::Drill { index }) => self.enter_level(index),
            Some(RowAction::Pick(sel)) => self.finish(Some(sel), ctx),
            None => {}
        }
    }
}

fn load_icon(icon: &IconSource) -> Option<egui::ColorImage> {
    match icon {
        IconSource::None => None,
        IconSource::File(p) => {
            let bytes = std::fs::read(p).ok()?;
            decode_bytes(&bytes)
        }
        IconSource::Bytes(b) => decode_bytes(b),
        IconSource::Pixels {
            width,
            height,
            rgba,
        } => {
            if rgba.len() != width * height * 4 {
                return None;
            }
            Some(egui::ColorImage::from_rgba_premultiplied(
                [*width, *height],
                rgba,
            ))
        }
    }
}

fn decode_bytes(bytes: &[u8]) -> Option<egui::ColorImage> {
    let dyn_img = image::load_from_memory(bytes).ok()?;
    // Downscale once at decode time: icons render at `icon_size` (28px) and
    // grid tiles at <= 200px, so uploading a 4000px photo as a GPU texture
    // wastes memory, upload bandwidth, and sampler cost every frame it is
    // drawn. 256px keeps 2x headroom for HiDPI tiles.
    const MAX_EDGE: u32 = 256;
    let dyn_img = if dyn_img.width() > MAX_EDGE || dyn_img.height() > MAX_EDGE {
        dyn_img.thumbnail(MAX_EDGE, MAX_EDGE)
    } else {
        dyn_img
    };
    let rgba = dyn_img.to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [w, h],
        rgba.as_raw(),
    ))
}

impl eframe::App for ModalApp {
    // Fully transparent clear: eframe's default (12,12,12,180) would leave
    // a dark veil across the transparent margin ring and rounded corners.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Options may still be arriving (run_streaming): fold them in first so
        // shortcuts and rendering see the current list.
        if self.poll_options(ctx) {
            return;
        }
        if self.loading {
            // Only Esc is meaningful while waiting for the first batch.
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.finish(None, ctx);
                return;
            }
            let t = self.modal.theme.clone();
            self.apply_style_once(ctx, &t);
            self.show_loading(ctx, &t);
            return;
        }

        // File-configured shortcuts win over the built-in keys so a user can
        // rebind them (e.g. make `Enter` do something else). Checked first.
        if self.fire_shortcut(ctx) {
            self.scroll_to_cursor = false;
            return;
        }

        let t = self.modal.theme.clone();

        // While the search field has text and focus, Left/Right/Home/End
        // belong to the text cursor. The list navigation shortcuts only take
        // them when the field is empty (or not focused), which keeps editing
        // and list/submenu movement from fighting over the same keys.
        let editing_text = self.modal.search.enabled
            && !self.query.is_empty()
            && ctx.memory(|m| m.has_focus(self.search_id));

        // Global keys — consumed under a single input lock instead of one
        // lock per key.
        let (
            esc,
            down,
            right,
            up,
            left,
            enter,
            home,
            end,
            page_down,
            page_up,
            ctrl_n,
            ctrl_p,
            ctrl_u,
            tab,
        ) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                !editing_text && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                !editing_text && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                !editing_text && i.consume_key(egui::Modifiers::NONE, egui::Key::Home),
                !editing_text && i.consume_key(egui::Modifiers::NONE, egui::Key::End),
                i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp),
                i.consume_key(egui::Modifiers::CTRL, egui::Key::N),
                i.consume_key(egui::Modifiers::CTRL, egui::Key::P),
                i.consume_key(egui::Modifiers::CTRL, egui::Key::U),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
            )
        });
        if ctrl_u {
            self.query.clear();
            self.cursor = 0;
        }
        // Tab completes the prefix the user is typing (`>sh` → `>show items`).
        // Consumed above, so the text field never inserts a tab.
        let mut tab_completed = false;
        if tab {
            if let Some(cp) = prefix::completion(&self.modal.compiled_prefixes, &self.query) {
                self.query = cp.prefix.clone();
                self.cursor = 0;
                tab_completed = true;
            } else if let Some(label) = self.focused_item_label() {
                // No prefix to finish: complete the search with the focused
                // row instead. Counted as a normal programmatic edit below so
                // the text cursor lands at the end of the label.
                self.query = label;
                self.cursor = 0;
                tab_completed = true;
            }
        }
        if esc {
            if self.nav_path.is_empty() {
                self.finish(None, ctx);
                return;
            }
            // Back out of the submenu instead of cancelling.
            self.exit_level();
        }

        // Backspace backs out of a submenu, but only when the search field
        // has nothing to delete: this runs before the TextEdit sees the key,
        // so a non-empty query must be left alone for editing.
        if self.query.is_empty()
            && !self.nav_path.is_empty()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace))
        {
            self.exit_level();
        }

        // Single filtered-list computation per frame: the cache refreshes
        // only when the query/config changed, and this clone is the only
        // per-frame allocation (a `Vec<usize>` memcpy, not per-option
        // Strings). It is also reused for the fit-to-content check below.
        let visible = self.visible_indices();
        if self.cursor >= visible.len() {
            self.cursor = visible.len().saturating_sub(1);
        }

        // Grid: Up/Down move a full row; List: every arrow moves one item.
        // (`list` arrows double as submenu navigation, see below.)
        let grid = matches!(self.modal.layout, Layout::Grid);
        let list = !grid;
        let row_step = if grid { self.last_columns.max(1) } else { 1 };

        if (down || ctrl_n) && !visible.is_empty() {
            self.cursor = if grid {
                (self.cursor + row_step).min(visible.len() - 1)
            } else {
                (self.cursor + 1) % visible.len()
            };
            self.scroll_to_cursor = self.modal.center_on_navigate;
        }
        if right && !visible.is_empty() {
            // List layout: Right enters a submenu, or steps to the next row
            // on leaves (flat modals have no parents, so their behavior is
            // unchanged). Grid keeps Right as sideways movement.
            if list {
                match self.activate(&visible, self.cursor) {
                    Some(RowAction::Drill { index }) => self.enter_level(index),
                    _ => {
                        self.cursor = (self.cursor + 1) % visible.len();
                        self.scroll_to_cursor = self.modal.center_on_navigate;
                    }
                }
            } else {
                self.cursor = (self.cursor + 1) % visible.len();
                self.scroll_to_cursor = self.modal.center_on_navigate;
            }
        }
        if (up || ctrl_p) && !visible.is_empty() {
            self.cursor = if grid {
                self.cursor.saturating_sub(row_step)
            } else {
                self.cursor.checked_sub(1).unwrap_or(visible.len() - 1)
            };
            self.scroll_to_cursor = self.modal.center_on_navigate;
        }
        if left && !visible.is_empty() {
            // List layout: Left backs out of a submenu, or steps to the
            // previous row at the root (flat modals: unchanged). Grid keeps
            // Left as sideways movement.
            if list && !self.nav_path.is_empty() {
                self.exit_level();
            } else {
                self.cursor = self.cursor.checked_sub(1).unwrap_or(visible.len() - 1);
                self.scroll_to_cursor = self.modal.center_on_navigate;
            }
        }
        if (home || page_up) && !visible.is_empty() {
            // Home jumps to the top; PageUp jumps 10 rows.
            self.cursor = if home {
                0
            } else {
                self.cursor.saturating_sub(10 * row_step.max(1))
            };
            self.scroll_to_cursor = self.modal.center_on_navigate;
        }
        if (end || page_down) && !visible.is_empty() {
            self.cursor = if end {
                visible.len() - 1
            } else {
                (self.cursor + 10 * row_step.max(1)).min(visible.len() - 1)
            };
            self.scroll_to_cursor = self.modal.center_on_navigate;
        }
        if enter {
            // A registered prefix confirmed with Enter runs its action and
            // closes with no pick — this wins over option activation.
            if let Some((cp, rest)) =
                prefix::match_prefix(&self.modal.compiled_prefixes, &self.query)
            {
                let cp = cp.clone();
                Self::run_prefix(&cp, &rest, &self.nav_path);
                self.finish(None, ctx);
                return;
            }
            if visible.is_empty() {
                // Command-palette style: return the typed text itself.
                if self.modal.allow_custom_input && !self.query.is_empty() {
                    let q = self.query.clone();
                    self.finish(
                        Some(ModalSelection {
                            id: q.clone(),
                            label: q.clone(),
                            index: 0,
                            path: self.nav_path.clone(),
                            query: q,
                            custom_input: true,
                            shortcut: None,
                        }),
                        ctx,
                    );
                    return;
                }
            } else {
                match self.activate(&visible, self.cursor) {
                    Some(RowAction::Pick(sel)) => {
                        self.finish(Some(sel), ctx);
                        return;
                    }
                    // Drilling clears the query; the panel below recomputes the
                    // visible list for the new level, so fall through.
                    Some(RowAction::Drill { index }) => self.enter_level(index),
                    None => {}
                }
            }
        }

        // Styling. egui retains the style across frames, so set it once:
        // cloning + installing the whole `Style` every frame invalidates
        // visuals at 60Hz for a theme that never changes mid-modal.
        self.apply_style_once(ctx, &t);

        // One input poll per frame; rows/tiles receive the bool instead of
        // each locking `ctx.input` themselves (O(n) locks per frame).
        let hover_fresh = ctx.input(|i| i.pointer.hover_pos()) != self.last_hover_pos;

        // Borderless: no stroke on the modal frame.
        let frame = egui::Frame::new()
            .fill(t.background)
            .corner_radius(t.corner_radius)
            .inner_margin(t.padding)
            .outer_margin(t.margin);

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            ui.style_mut().override_text_style = Some(egui::TextStyle::Body);
            // Breadcrumb for drilled-down submenus (`trail` is owned, so the
            // jump buttons below may mutate navigation freely).
            if !self.nav_path.is_empty() {
                let trail = self.trail_labels();
                ui.horizontal(|ui| {
                    if ui.small_button("‹").clicked() {
                        self.exit_level();
                    }
                    if ui.small_button("Top").clicked() {
                        self.nav_to_depth(0);
                    }
                    for (depth, label) in trail.iter().enumerate() {
                        ui.label("/");
                        if ui.small_button(label).clicked() {
                            self.nav_to_depth(depth + 1);
                        }
                    }
                });
                ui.add_space(4.0);
            }
            // Search field, wrapped in a themed frame (background,
            // radius). The TextEdit itself is frameless so the frame is the
            // only box drawn.
            if self.modal.search.enabled {
                let search_font = egui::FontId::proportional(t.effective_search_font_size());
                let resp = egui::Frame::new()
                    .fill(t.search_background)
                    .corner_radius(t.effective_search_radius())
                    .inner_margin(egui::Margin::symmetric(10, 7))
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .id(self.search_id)
                                .hint_text(
                                    egui::RichText::new(&self.modal.search.placeholder)
                                        .color(t.search_hint),
                                )
                                .hint_text_font(search_font.clone())
                                .font(search_font)
                                .text_color(t.search_text)
                                .frame(false)
                                .background_color(egui::Color32::TRANSPARENT)
                                .desired_width(f32::INFINITY)
                                .margin(egui::Vec2::ZERO),
                        )
                    })
                    .inner;
                if (self.modal.search.autofocus && !self.focused_once)
                    || self.query.is_empty() && !self.focused_once
                {
                    resp.request_focus();
                    self.focused_once = true;
                }
                if resp.changed() {
                    self.cursor = 0;
                }
                // Tab replaced the buffer text before this field was drawn, so
                // move the editing cursor to the end of the completed prefix.
                if tab_completed {
                    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), self.search_id) {
                        let end = egui::text::CCursor::new(self.query.chars().count());
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(end)));
                        state.store(ui.ctx(), self.search_id);
                    }
                    resp.request_focus();
                }
                ui.add_space(8.0);
                // Active prefix: Enter runs it. Partial prefix: Tab completes
                // it to the shortest registered match.
                let mut hints: Vec<String> = Vec::new();
                if let Some((p, _)) =
                    prefix::match_prefix(&self.modal.compiled_prefixes, &self.query)
                {
                    hints.push(format!("↵ run \"{}\"", p.hint));
                }
                if let Some(c) = prefix::completion(&self.modal.compiled_prefixes, &self.query) {
                    hints.push(format!("Tab ⇥ complete \"{}\"", c.prefix));
                } else if let Some(label) = self.focused_item_label() {
                    hints.push(format!("Tab ⇥ complete \"{label}\""));
                }
                if !hints.is_empty() {
                    ui.label(
                        egui::RichText::new(hints.join("  ·  "))
                            .size(t.font_size * 0.8)
                            .color(t.dim_text),
                    );
                    ui.add_space(6.0);
                }
            } else if self.modal.search.autofocus && !self.focused_once {
                // No search field: nothing to focus; mark done so we don't retry.
                self.focused_once = true;
            }

            // NOTE: `visible` was computed before the search field could
            // edit `query` this frame; refresh so the list never lags one
            // frame behind typing.
            let visible = self.visible_indices();
            if visible.is_empty() {
                // An empty query is the "nothing filtered yet" state (e.g. a
                // hidden list or a loading modal), not a failed search: stay
                // blank instead of claiming "No matches".
                if self.shows_empty_message() {
                    let msg = self.modal.empty_message.clone();
                    ui.colored_label(t.dim_text, &msg);
                    if self.modal.allow_custom_input && !self.query.is_empty() {
                        ui.colored_label(t.dim_text, format!("Enter ↵: use \"{}\"", self.query));
                    }
                }
            } else if matches!(self.modal.layout, Layout::Grid) {
                self.show_grid(ctx, ui, &visible, &t, hover_fresh);
            } else {
                self.show_list(ctx, ui, &visible, &t, hover_fresh);
            }
            if let Some(hint) = self.modal.footer_hint.clone() {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(hint)
                        .size(t.font_size * 0.78)
                        .color(t.dim_text),
                );
            }
        });

        // Request consumed: only keyboard moves re-center.
        self.scroll_to_cursor = false;

        // Shrink the window width to the measured content (never below
        // 320px, never above the theme width). The measurement must stay
        // put for consecutive updates first: the first multi-pass layouts
        // report transient widths that must never resize the window.
        // Skipped until the first real measurement so it never flashes narrow.
        // The search field may have edited `query` inside the panel above,
        // so re-check the cache (no clone: emptiness only) instead of
        // reusing the pre-edit `visible`.
        // Fit to the current level's FULL list only: a filtered subset
        // measures narrower, which would shrink the window, recompute fewer
        // grid columns, and trap the restored full list in that narrow layout
        // after clearing.
        self.refresh_filter_cache();
        let full_list = self.filter_visible.len() == self.level_len();
        if self.modal.fit_width_to_content
            && self.last_content_width > 0.0
            && full_list
            && !self.filter_visible.is_empty()
        {
            if (self.last_content_width - self.fit_candidate).abs() <= 1.0 {
                self.fit_stable_frames += 1;
            } else {
                self.fit_candidate = self.last_content_width;
                self.fit_stable_frames = 0;
            }
            if self.fit_stable_frames >= 1 {
                let want = (self.last_content_width + t.padding * 2.0 + t.margin * 2.0)
                    .clamp(320.0, t.width);
                if (want - self.last_sent_width).abs() > 1.0 {
                    // New target: fresh retry budget.
                    self.last_sent_width = want;
                    self.fit_send_attempts = 0;
                }
                if (ctx.screen_rect().width() - want).abs() <= 1.0 {
                    // Window manager applied it: nothing to do.
                    self.fit_send_attempts = 0;
                } else {
                    if self.fit_send_attempts < MAX_FIT_SENDS {
                        self.fit_send_attempts += 1;
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                            want, t.height,
                        )));
                    }
                    // else: the WM keeps ignoring us (e.g. a tiling
                    // compositor) — stop sending instead of spamming.
                    if self.fit_send_attempts < MAX_FIT_SENDS {
                        // Keep frames coming until the resize lands. The
                        // first `InnerSize` can be lost while the window is
                        // still mapping; with send-once dedup that stuck the
                        // modal at full width with the grid centered in side
                        // margins. Bounded: stops once applied or exhausted.
                        ctx.request_repaint();
                    }
                }
            } else if self.fit_send_attempts < MAX_FIT_SENDS {
                // Measurement not settled yet — make sure the next frame
                // comes so it can settle (same bound, can't spin forever).
                self.fit_send_attempts += 1;
                ctx.request_repaint();
            }
        } else {
            self.fit_stable_frames = 0;
            self.fit_send_attempts = 0;
        }
        self.last_hover_pos = ctx.input(|i| i.pointer.hover_pos());

        // No blanket `request_repaint_after`: that kept the app waking 5x/sec
        // at idle (CPU/battery). egui already repaints on input, focus, and
        // scroll-animation frames; textures appear on the frame their
        // `load_texture` completes.
    }
}

#[cfg(test)]
mod tests;
