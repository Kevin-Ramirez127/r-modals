use crate::{IconSource, Layout, Modal, ModalOption, ModalSelection};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
        let query = modal.initial_query.clone();
        let mut app = Self {
            modal,
            query,
            cursor: 0,
            textures: HashMap::new(),
            result,
            search_id: egui::Id::new("quick-modals-search"),
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
        if let Some(want) = &app.modal.default_selected {
            if let Some(pos) = app
                .filter_visible
                .iter()
                .position(|&i| app.level_slice().get(i).is_some_and(|o| &o.id == want))
            {
                app.cursor = pos;
            }
        }
        app
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
        if self.filter_initialized
            && self.filter_query == self.query
            && self.filter_case_sensitive == case_sensitive
            && self.filter_fuzzy == fuzzy
            && self.filter_enabled == enabled
            && self.filter_max == max
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
        let mut visible: Vec<usize> = if !enabled || query.is_empty() {
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
        self.filter_initialized = true;
    }

    fn visible_indices(&mut self) -> Vec<usize> {
        self.refresh_filter_cache();
        self.filter_visible.clone()
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
        }))
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
                    egui::Grid::new("quick-modals-tiles")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_valid_png() {
        let bytes = include_bytes!("../assets/firefox.png");
        assert!(decode_bytes(bytes).is_some());
        let img = decode_bytes(bytes).unwrap();
        assert!(img.width() > 0 && img.height() > 0);
    }

    #[test]
    fn decode_invalid_bytes_returns_none() {
        assert!(decode_bytes(b"not an image").is_none());
        assert!(decode_bytes(include_bytes!("../README.md")).is_none());
    }

    #[test]
    fn load_icon_variants() {
        // File (ships with repo)
        assert!(load_icon(&IconSource::File("assets/firefox.png".into())).is_some());
        // Missing file -> None (graceful)
        assert!(load_icon(&IconSource::File("assets/does-not-exist.png".into())).is_none());
        // Bytes
        let bytes = include_bytes!("../assets/terminal.png").to_vec();
        assert!(load_icon(&IconSource::Bytes(bytes)).is_some());
        assert!(load_icon(&IconSource::Bytes(b"junk".to_vec())).is_none());
        // Pixels: valid + length mismatch
        assert!(load_icon(&IconSource::Pixels {
            width: 2,
            height: 2,
            rgba: vec![255; 2 * 2 * 4],
        })
        .is_some());
        assert!(load_icon(&IconSource::Pixels {
            width: 2,
            height: 2,
            rgba: vec![255; 3],
        })
        .is_none());
        assert!(load_icon(&IconSource::None).is_none());
    }

    fn headless_input(time: f64) -> egui::RawInput {
        headless_input_w(time, 600.0)
    }

    fn headless_input_w(time: f64, width: f32) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 500.0),
            )),
            time: Some(time),
            ..Default::default()
        }
    }

    fn key_event(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// egui mechanism check with the same nesting as `show_grid`:
    /// `scroll_to_me(Center)` must bring an off-screen tile to the center.
    #[test]
    fn scroll_to_me_centers_grid_tile() {
        let ctx = egui::Context::default();
        let mut tile_rect = egui::Rect::NOTHING;
        let mut inner_rect = egui::Rect::NOTHING;
        let mut offset = egui::Vec2::ZERO;
        let mut content = egui::Vec2::ZERO;
        for pass in 0..10 {
            // Large time steps so the scroll animation completes.
            let t = pass as f64 * 0.25;
            let _ = ctx.run(headless_input(t), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let mut wanted = egui::Rect::NOTHING;
                    let out = egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            egui::Grid::new("test-tiles")
                                .num_columns(4)
                                .spacing([4.0, 4.0])
                                .show(ui, |ui| {
                                    for i in 0..30 {
                                        let r = egui::Frame::new()
                                            .inner_margin(4.0)
                                            .show(ui, |ui| {
                                                ui.set_min_size(egui::Vec2::splat(64.0));
                                                ui.set_max_size(egui::Vec2::splat(64.0));
                                                ui.label(format!("tile {i}"));
                                            })
                                            .response;
                                        if pass == 1 && i == 15 {
                                            r.scroll_to_me(Some(egui::Align::Center));
                                        }
                                        if pass == 9 && i == 15 {
                                            wanted = r.rect;
                                        }
                                        if (i + 1) % 4 == 0 {
                                            ui.end_row();
                                        }
                                    }
                                });
                        });
                    if pass == 9 {
                        tile_rect = wanted;
                        inner_rect = out.inner_rect;
                        offset = out.state.offset;
                        content = out.content_size;
                    }
                });
            });
        }
        assert!(
            content.y > inner_rect.height(),
            "need overflow for this test, content={content:?} inner={inner_rect:?}"
        );
        assert!(offset.y > 0.0, "expected scroll, offset={offset:?}");
        let tile_c = tile_rect.center().y;
        let view_c = inner_rect.center().y;
        // egui keeps a few px of internal inset, so exact equality is too
        // strict; anything well under half a tile proves centering.
        assert!(
            (tile_c - view_c).abs() < 8.0,
            "tile not centered: tile_c={tile_c} view_c={view_c}"
        );
    }

    fn grid_app(n: usize, center: bool) -> (egui::Context, ModalApp, eframe::Frame) {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let modal = crate::Modal::new("grid")
            .layout(crate::Layout::Grid)
            .center_on_navigate(center)
            .options((0..n).map(|i| {
                crate::ModalOption::new(format!("id{i}"), format!("Item {i}")).icon_pixels(
                    8,
                    8,
                    vec![200; 8 * 8 * 4],
                )
            }));
        let result = Arc::new(Mutex::new(None));
        let app = ModalApp::new(&cc, modal, result);
        let frame = eframe::Frame::_new_kittest();
        (ctx, app, frame)
    }

    /// Drive the real app headlessly: arrows must move the cursor even with
    /// the search field focused, and the grid must scroll to follow.
    /// Grid Down moves a full row (clamped at the end).
    #[test]
    fn keyboard_navigation_centers_selected_tile() {
        use eframe::App as _;
        let (ctx, mut app, mut frame) = grid_app(30, true);
        let mut t = 0.0;
        for _ in 0..3 {
            t += 1.0 / 60.0;
            let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
        }
        assert_eq!(app.cursor, 0);
        let cols = app.last_columns;
        assert!(cols >= 2, "need multiple columns for this test");
        for _ in 0..8 {
            t += 1.0 / 60.0;
            let mut inp = headless_input(t);
            inp.events.push(key_event(egui::Key::ArrowDown));
            let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        }
        assert_eq!(app.cursor, (8 * cols).min(29));
        assert!(
            app.last_scroll_offset.y > 0.0,
            "expected grid to scroll on keyboard nav, offset={:?}",
            app.last_scroll_offset
        );
    }

    /// Opt-out: no auto-scroll when disabled.
    #[test]
    fn center_on_navigate_false_keeps_scroll() {
        use eframe::App as _;
        let (ctx, mut app, mut frame) = grid_app(30, false);
        let mut t = 0.0;
        for _ in 0..3 {
            t += 1.0 / 60.0;
            let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
        }
        let cols = app.last_columns;
        for _ in 0..8 {
            t += 1.0 / 60.0;
            let mut inp = headless_input(t);
            inp.events.push(key_event(egui::Key::ArrowDown));
            let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        }
        assert_eq!(app.cursor, (8 * cols).min(29));
        assert_eq!(app.last_scroll_offset, egui::Vec2::ZERO);
    }

    fn inner_sizes(out: &egui::FullOutput) -> Vec<egui::Vec2> {
        out.viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|vo| {
                vo.commands
                    .iter()
                    .filter_map(|c| match c {
                        egui::ViewportCommand::InnerSize(s) => Some(*s),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Fit off (default): no resize commands are ever sent.
    #[test]
    fn fit_width_off_sends_no_resize() {
        use eframe::App as _;
        let (ctx, mut app, mut frame) = grid_app(30, true);
        let mut t = 0.0;
        let mut sizes = Vec::new();
        for _ in 0..6 {
            t += 1.0 / 60.0;
            let out = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
            sizes.extend(inner_sizes(&out));
        }
        assert!(sizes.is_empty(), "unexpected resize: {sizes:?}");
    }

    /// Fit on: a narrow grid shrinks the window below the theme width.
    #[test]
    fn fit_width_shrinks_window_to_content() {
        use eframe::App as _;
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let modal = crate::Modal::new("fit")
            .layout(crate::Layout::Grid)
            .fit_width_to_content(true)
            .theme(crate::Theme::from_css("tile { width: 200px; height: 200px; }").unwrap())
            .options((0..2).map(|i| {
                crate::ModalOption::new(format!("id{i}"), format!("Item {i}")).icon_pixels(
                    8,
                    8,
                    vec![200; 8 * 8 * 4],
                )
            }));
        let result = Arc::new(Mutex::new(None));
        let mut app = ModalApp::new(&cc, modal, result);
        let mut frame = eframe::Frame::_new_kittest();
        let mut t = 0.0;
        let mut sizes = Vec::new();
        for _ in 0..6 {
            t += 1.0 / 60.0;
            let out = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
            sizes.extend(inner_sizes(&out));
        }
        assert!(!sizes.is_empty(), "expected an InnerSize command");
        let w = sizes.last().unwrap().x;
        assert!(
            (320.0..560.0).contains(&w),
            "expected shrink-to-fit width, got {w}"
        );
    }

    /// Fit must ignore the transient first-pass measurement: multi-pass
    /// layout reports a narrow width before settling. Regression test:
    /// no resize below the settled width may ever be sent.
    #[test]
    fn fit_ignores_transient_first_pass() {
        use eframe::App as _;
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let modal = crate::Modal::new("Pick an image")
            .layout(crate::Layout::Grid)
            .fit_width_to_content(true)
            .theme(
                crate::Theme::from_css(
                    "modal { width: 560px; border-radius: 12px; }
                     tile { width: 128px; height: 128px; }",
                )
                .unwrap(),
            )
            .options((0..6).map(|i| {
                crate::ModalOption::new(format!("id{i}"), format!("Item {i}")).icon_pixels(
                    8,
                    8,
                    vec![200; 8 * 8 * 4],
                )
            }));
        let result = Arc::new(Mutex::new(None));
        let mut app = ModalApp::new(&cc, modal, result);
        let mut frame = eframe::Frame::_new_kittest();
        let mut t = 0.0;
        let mut sizes = Vec::new();
        for _ in 0..6 {
            t += 1.0 / 60.0;
            let out = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
            sizes.extend(inner_sizes(&out));
        }
        assert!(!sizes.is_empty(), "expected an InnerSize command");
        for s in &sizes {
            assert!(
                s.x >= 400.0,
                "transient narrow resize must never be sent: {sizes:?}"
            );
        }
    }

    /// Hover follows fresh mouse movement…
    #[test]
    fn hover_follows_fresh_movement() {
        use eframe::App as _;
        let (ctx, mut app, mut frame) = grid_app(30, true);
        let mut t = 0.0;
        for _ in 0..3 {
            t += 1.0 / 60.0;
            let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
        }
        // Arrow away from tile 0 with no mouse involved.
        t += 1.0 / 60.0;
        let mut inp = headless_input(t);
        inp.events.push(key_event(egui::Key::ArrowRight));
        let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.cursor, 1);
        // Moving the pointer onto tile 0 takes the cursor back.
        t += 1.0 / 60.0;
        let mut inp = headless_input(t);
        inp.events
            .push(egui::Event::PointerMoved(egui::Pos2::new(80.0, 130.0)));
        let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.cursor, 0);
    }

    fn fit_grid_app() -> (egui::Context, ModalApp, eframe::Frame) {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let modal = crate::Modal::new("grid")
            .layout(crate::Layout::Grid)
            .fit_width_to_content(true)
            .options((0..30).map(|i| {
                crate::ModalOption::new(format!("id{i}"), format!("Item {i}")).icon_pixels(
                    8,
                    8,
                    vec![200; 8 * 8 * 4],
                )
            }));
        let result = Arc::new(Mutex::new(None));
        let app = ModalApp::new(&cc, modal, result);
        let frame = eframe::Frame::_new_kittest();
        (ctx, app, frame)
    }

    /// Regression: filtering a fit-to-content grid must not shrink the
    /// window, and clearing the filter must restore the initial columns.
    /// Previously the window shrank to the filtered content, recomputed
    /// e.g. 2 columns, and the narrow window trapped the restored full
    /// list in that 2-column grid.
    #[test]
    fn grid_filter_keeps_window_and_restores_columns() {
        use eframe::App as _;
        let (ctx, mut app, mut frame) = fit_grid_app();
        let mut t = 0.0;
        for _ in 0..6 {
            t += 1.0 / 60.0;
            let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
        }
        let initial = app.last_columns;
        assert!(initial >= 2, "need multiple columns, got {initial}");
        let settled = app.last_sent_width;
        assert!(settled > 0.0, "expected the fit window to settle");
        // Filter down to a single tile ("Item 29" matches nothing else).
        app.query = "Item 29".to_string();
        let mut sizes = Vec::new();
        for _ in 0..6 {
            t += 1.0 / 60.0;
            let out = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
            sizes.extend(inner_sizes(&out));
        }
        assert_eq!(app.filter_visible.len(), 1);
        assert_eq!(
            app.last_columns, initial,
            "columns must not collapse while filtered"
        );
        for s in &sizes {
            assert!(
                s.x >= settled - 1.0,
                "filter must not shrink the window: settled={settled} sizes={sizes:?}"
            );
        }
        // Clearing the filter restores the full grid at its initial width.
        app.query.clear();
        for _ in 0..6 {
            t += 1.0 / 60.0;
            let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
        }
        assert_eq!(app.filter_visible.len(), 30);
        assert_eq!(
            app.last_columns, initial,
            "clearing the filter must restore the initial columns"
        );
    }

    /// While filtered, the grid reuses the remembered full-list columns
    /// even if the available width narrows (e.g. the fitted window was
    /// briefly narrow): the layout must not get stuck at 2 columns.
    #[test]
    fn grid_reuses_initial_columns_when_narrowed_while_filtered() {
        use eframe::App as _;
        let (ctx, mut app, mut frame) = fit_grid_app();
        let mut t = 0.0;
        for _ in 0..6 {
            t += 1.0 / 60.0;
            let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
        }
        let initial = app.last_columns;
        assert!(initial >= 2, "need multiple columns, got {initial}");
        // Narrow viewport + active filter: columns must hold.
        app.query = "Item 29".to_string();
        for _ in 0..6 {
            t += 1.0 / 60.0;
            let _ = ctx.run(headless_input_w(t, 340.0), |ctx| {
                app.update(ctx, &mut frame)
            });
        }
        assert_eq!(
            app.last_columns, initial,
            "filtered grid must reuse the remembered columns"
        );
    }

    /// Fit must retry the resize until the window actually matches: the
    /// first `InnerSize` can be lost while the window is still mapping, and
    /// send-once dedup then stuck the modal wide (grid centered with side
    /// margins). Headless the window never applies it (stays 600px), so
    /// every settled frame must re-send the same target width.
    #[test]
    fn fit_retries_resize_until_window_matches() {
        use eframe::App as _;
        let (ctx, mut app, mut frame) = fit_grid_app();
        // fit_grid_app uses 112px tiles: target width is below 600px, so the
        // (never-applied, headless) resize must be re-sent, not sent once.
        let mut t = 0.0;
        let mut sizes = Vec::new();
        for _ in 0..6 {
            t += 1.0 / 60.0;
            let out = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
            sizes.extend(inner_sizes(&out));
        }
        assert!(
            sizes.len() >= 4,
            "expected the fit resize to be retried while unapplied, got {sizes:?}"
        );
        let first = sizes[0].x;
        for s in &sizes {
            assert!(
                (s.x - first).abs() <= 1.0,
                "retries must repeat the same target, got {sizes:?}"
            );
        }
    }

    fn nested_app() -> (egui::Context, ModalApp, eframe::Frame) {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let modal = crate::Modal::new("settings").options([
            crate::ModalOption::new("disp", "Display").sub_options([
                crate::ModalOption::new("res", "Resolution").sub_options([
                    crate::ModalOption::new("1080p", "1920×1080"),
                    crate::ModalOption::new("4k", "3840×2160"),
                ]),
                crate::ModalOption::new("bright", "Brightness").sub_options([
                    crate::ModalOption::new("bright-low", "Low"),
                    crate::ModalOption::new("bright-high", "High"),
                ]),
            ]),
            crate::ModalOption::new("snd", "Sound")
                .sub_option(crate::ModalOption::new("vol", "Volume")),
        ]);
        let result = Arc::new(Mutex::new(None));
        let app = ModalApp::new(&cc, modal, result);
        let frame = eframe::Frame::_new_kittest();
        (ctx, app, frame)
    }

    fn enter_event(time: f64) -> egui::RawInput {
        let mut inp = headless_input(time);
        inp.events.push(key_event(egui::Key::Enter));
        inp
    }

    fn esc_event(time: f64) -> egui::RawInput {
        let mut inp = headless_input(time);
        inp.events.push(key_event(egui::Key::Escape));
        inp
    }

    /// Enter on a parent drills in; Enter on a leaf picks it with the full
    /// index path.
    #[test]
    fn submenu_drill_and_leaf_pick() {
        use eframe::App as _;
        let (ctx, mut app, mut frame) = nested_app();
        let mut t = 0.0;
        for _ in 0..3 {
            t += 1.0 / 60.0;
            let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
        }
        assert_eq!(app.filter_visible.len(), 2);
        // Cursor 0 = Display (parent) → drill.
        t += 1.0 / 60.0;
        let _ = ctx.run(enter_event(t), |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.nav_path, vec![0]);
        assert!(app.query.is_empty());
        assert_eq!(app.filter_visible.len(), 2); // Resolution, Brightness
                                                 // Move to Brightness (1) → drill → Low/High.
        app.cursor = 1;
        t += 1.0 / 60.0;
        let _ = ctx.run(enter_event(t), |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.nav_path, vec![0, 1]);
        assert_eq!(app.filter_visible.len(), 2);
        // Pick the first leaf.
        t += 1.0 / 60.0;
        let _ = ctx.run(enter_event(t), |ctx| app.update(ctx, &mut frame));
        let sel = app.result.lock().unwrap().clone().expect("leaf picked");
        assert_eq!(sel.id, "bright-low");
        assert_eq!(sel.label, "Low");
        assert_eq!(sel.index, 0);
        assert_eq!(sel.path, vec![0, 1, 0]);
    }

    /// Esc backs out one level; Esc at the root still cancels.
    #[test]
    fn submenu_esc_goes_up_then_cancels() {
        use eframe::App as _;
        let (ctx, mut app, mut frame) = nested_app();
        let mut t = 0.0;
        for _ in 0..3 {
            t += 1.0 / 60.0;
            let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
        }
        app.enter_level(0);
        app.enter_level(1);
        assert_eq!(app.nav_path, vec![0, 1]);
        t += 1.0 / 60.0;
        let _ = ctx.run(esc_event(t), |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.nav_path, vec![0]);
        assert!(app.result.lock().unwrap().is_none());
        t += 1.0 / 60.0;
        let _ = ctx.run(esc_event(t), |ctx| app.update(ctx, &mut frame));
        assert!(app.nav_path.is_empty());
    }

    /// Backspace with an empty query backs out; with text it is left alone
    /// for the search field (never consumed for navigation).
    #[test]
    fn submenu_backspace_goes_up_only_on_empty_query() {
        use eframe::App as _;
        let (ctx, mut app, mut frame) = nested_app();
        let mut t = 0.0;
        for _ in 0..3 {
            t += 1.0 / 60.0;
            let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
        }
        app.enter_level(0);
        // Non-empty query: Backspace is left for the search field — nav and
        // query are untouched by navigation handling. (Whether the headless
        // TextEdit itself deletes a char is egui harness behavior; the real
        // backend edits normally. What matters here is we never consume it.)
        app.query = "Res".to_string();
        t += 1.0 / 60.0;
        let mut inp = headless_input(t);
        inp.events.push(key_event(egui::Key::Backspace));
        let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.nav_path, vec![0]);
        assert_eq!(app.query, "Res");
        // Empty query: Backspace backs out.
        app.query.clear();
        app.filter_initialized = false;
        t += 1.0 / 60.0;
        let mut inp = headless_input(t);
        inp.events.push(key_event(egui::Key::Backspace));
        let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        assert!(app.nav_path.is_empty());
    }

    /// A filtered parent acts as a gateway: it lists while a descendant
    /// matches, and drilling into it clears the query.
    #[test]
    fn submenu_filter_gateway_and_drill_clears_query() {
        use eframe::App as _;
        let (ctx, mut app, mut frame) = nested_app();
        let mut t = 0.0;
        for _ in 0..3 {
            t += 1.0 / 60.0;
            let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
        }
        app.query = "3840".to_string();
        t += 1.0 / 60.0;
        let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.filter_visible, vec![0]); // Display, via 3840×2160
        t += 1.0 / 60.0;
        let _ = ctx.run(enter_event(t), |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.nav_path, vec![0]);
        assert!(app.query.is_empty());
    }

    /// Nav helpers: leaves never drill, depths jump, caches track the level.
    #[test]
    fn submenu_nav_helpers() {
        let (ctx, mut app, _) = nested_app();
        let _ = ctx;
        assert_eq!(app.level_len(), 2);
        app.enter_level(1); // Sound (parent) → ok
        assert_eq!(app.nav_path, vec![1]);
        assert_eq!(app.level_len(), 1);
        assert_eq!(app.level_icon_keys.len(), 1);
        assert_eq!(app.level_lowered.len(), 1);
        app.enter_level(0); // Volume (leaf) → no-op
        assert_eq!(app.nav_path, vec![1]);
        assert!(app.exit_level()); // pops → true
        assert!(app.nav_path.is_empty());
        assert!(!app.exit_level()); // root → false
        app.enter_level(0);
        app.enter_level(0);
        assert_eq!(app.nav_path, vec![0, 0]);
        app.nav_to_depth(1);
        assert_eq!(app.nav_path, vec![0]);
        app.nav_to_depth(0);
        assert!(app.nav_path.is_empty());
        app.nav_to_depth(5); // beyond depth → no-op
        assert!(app.nav_path.is_empty());
        assert_eq!(
            app.trail_labels(),
            Vec::<String>::new(),
            "root has no trail"
        );
        app.enter_level(0);
        assert_eq!(app.trail_labels(), vec!["Display".to_string()]);
    }

    /// List arrows double as submenu navigation: Right drills into the
    /// highlighted parent (next row on leaves), Left backs out (previous
    /// row at the root). Grid keeps arrows as pure movement.
    #[test]
    fn submenu_list_arrows_navigate() {
        use eframe::App as _;
        let (ctx, mut app, mut frame) = nested_app();
        let mut t = 0.0;
        for _ in 0..3 {
            t += 1.0 / 60.0;
            let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
        }
        // Cursor 0 = Display (parent): Right drills in.
        t += 1.0 / 60.0;
        let mut inp = headless_input(t);
        inp.events.push(key_event(egui::Key::ArrowRight));
        let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.nav_path, vec![0]);
        assert_eq!(app.filter_visible.len(), 2);
        // Left backs out to the root.
        t += 1.0 / 60.0;
        let mut inp = headless_input(t);
        inp.events.push(key_event(egui::Key::ArrowLeft));
        let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        assert!(app.nav_path.is_empty());
        // At the root Left keeps its old meaning: previous row (wraps).
        assert_eq!(app.cursor, 0);
        t += 1.0 / 60.0;
        let mut inp = headless_input(t);
        inp.events.push(key_event(egui::Key::ArrowLeft));
        let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.cursor, 1);
        // Cursor 1 = Sound (parent): Right drills in (single child).
        t += 1.0 / 60.0;
        let mut inp = headless_input(t);
        inp.events.push(key_event(egui::Key::ArrowRight));
        let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.nav_path, vec![1]);
        // On the leaf (Volume) Right steps (single row: stays, no drill).
        t += 1.0 / 60.0;
        let mut inp = headless_input(t);
        inp.events.push(key_event(egui::Key::ArrowRight));
        let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.nav_path, vec![1]);
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn stationary_pointer_does_not_block_keyboard() {
        use eframe::App as _;
        let (ctx, mut app, mut frame) = grid_app(30, true);
        let mut t = 0.0;
        for _ in 0..3 {
            t += 1.0 / 60.0;
            let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
        }
        let cols = app.last_columns;
        // Park the pointer over tile 0 and leave it there.
        t += 1.0 / 60.0;
        let mut inp = headless_input(t);
        inp.events
            .push(egui::Event::PointerMoved(egui::Pos2::new(80.0, 130.0)));
        let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.cursor, 0);
        // ArrowDown with the mouse still: keyboard must win.
        t += 1.0 / 60.0;
        let mut inp = headless_input(t);
        inp.events.push(key_event(egui::Key::ArrowDown));
        let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        assert_eq!(app.cursor, cols);
    }
}

impl eframe::App for ModalApp {
    // Fully transparent clear: eframe's default (12,12,12,180) would leave
    // a dark veil across the transparent margin ring and rounded corners.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let t = self.modal.theme.clone();

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
        ) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Home),
                i.consume_key(egui::Modifiers::NONE, egui::Key::End),
                i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp),
                i.consume_key(egui::Modifiers::CTRL, egui::Key::N),
                i.consume_key(egui::Modifiers::CTRL, egui::Key::P),
                i.consume_key(egui::Modifiers::CTRL, egui::Key::U),
            )
        });
        if ctrl_u {
            self.query.clear();
            self.cursor = 0;
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
        if !self.style_applied {
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
                ui.add_space(8.0);
            } else if self.modal.search.autofocus && !self.focused_once {
                // No search field: nothing to focus; mark done so we don't retry.
                self.focused_once = true;
            }

            // NOTE: `visible` was computed before the search field could
            // edit `query` this frame; refresh so the list never lags one
            // frame behind typing.
            let visible = self.visible_indices();
            if visible.is_empty() {
                let msg = self.modal.empty_message.clone();
                ui.colored_label(t.dim_text, &msg);
                if self.modal.allow_custom_input && !self.query.is_empty() {
                    ui.colored_label(t.dim_text, format!("Enter ↵: use \"{}\"", self.query));
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
