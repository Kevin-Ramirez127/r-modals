use crate::{filter_options, IconSource, Layout, Modal, ModalSelection};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    /// Column count of the last rendered grid (drives row-wise Up/Down).
    last_columns: usize,
}

impl ModalApp {
    pub(crate) fn new(
        _cc: &eframe::CreationContext<'_>,
        modal: Modal,
        result: Arc<Mutex<Option<ModalSelection>>>,
    ) -> Self {
        let query = modal.initial_query.clone();
        Self {
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
            last_columns: 1,
        }
    }

    fn visible_indices(&self) -> Vec<usize> {
        if !self.modal.search.enabled {
            return (0..self.modal.options.len()).collect();
        }
        filter_options(
            &self.modal.options,
            &self.query,
            self.modal.search.case_sensitive,
        )
    }

    fn choose(&self, visible: &[usize], pos: usize) -> Option<ModalSelection> {
        let &opt_idx = visible.get(pos)?;
        let opt = &self.modal.options[opt_idx];
        Some(ModalSelection {
            id: opt.id.clone(),
            label: opt.label.clone(),
            index: opt_idx,
            query: self.query.clone(),
        })
    }

    fn finish(&self, sel: Option<ModalSelection>, ctx: &egui::Context) {
        if let Ok(mut g) = self.result.lock() {
            *g = sel;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// True when the pointer moved since the last frame: only fresh mouse
    /// movement may take the cursor away from keyboard navigation.
    fn hover_fresh(&self, ctx: &egui::Context) -> bool {
        ctx.input(|i| i.pointer.hover_pos()) != self.last_hover_pos
    }

    fn icon_texture(
        &mut self,
        ctx: &egui::Context,
        option_id: &str,
        icon: &IconSource,
    ) -> Option<egui::TextureHandle> {
        let key = icon.cache_key(option_id)?;
        if let Some(t) = self.textures.get(&key) {
            return Some(t.clone());
        }
        let img = load_icon(icon)?;
        let tex = ctx.load_texture(&key, img, egui::TextureOptions::LINEAR);
        self.textures.insert(key.clone(), tex.clone());
        Some(tex)
    }

    /// List rows: thumbnail followed by label + description.
    fn show_list(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        visible: &[usize],
        t: &crate::Theme,
    ) {
        let out = egui::ScrollArea::vertical()
            .max_height(t.height - 110.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = t.item_spacing;
                for (pos, &opt_idx) in visible.iter().enumerate() {
                    let opt = &self.modal.options[opt_idx].clone();
                    let selected = pos == self.cursor;
                    let row_fill = if selected { t.selected } else { t.surface };

                    let row = egui::Frame::new()
                        .fill(row_fill)
                        .corner_radius(t.effective_tile_radius())
                        .inner_margin(egui::Margin::symmetric(8, 6))
                        .show(ui, |ui| {
                            // Stretch rows full-width, unless fitting the
                            // window to the content (then rows size to text).
                            if !self.modal.fit_width_to_content {
                                ui.set_width(ui.available_width());
                            }
                            ui.horizontal(|ui| {
                                // Icon (cached texture or placeholder box).
                                let tex = self.icon_texture(ctx, &opt.id, &opt.icon);
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
                                    ui.label(
                                        egui::RichText::new(&opt.label)
                                            .size(t.font_size)
                                            .color(t.text),
                                    );
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
                            });
                        });

                    // Keyboard navigation: center the selected row.
                    if self.scroll_to_cursor && pos == self.cursor {
                        row.response.scroll_to_me(Some(egui::Align::Center));
                    }
                    // Click + hover select (hover only on fresh mouse
                    // movement, so it never fights keyboard navigation).
                    if row.response.hovered() && self.hover_fresh(ctx) {
                        self.cursor = pos;
                    }
                    if row.response.clicked() {
                        let sel = self.choose(visible, pos);
                        self.finish(sel, ctx);
                        return;
                    }
                }
            });
        self.last_scroll_offset = out.state.offset;
        self.last_content_width = out.content_size.x;
    }

    /// Grid squares: each tile IS the image (no text, tooltip shows the label).
    fn show_grid(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        visible: &[usize],
        t: &crate::Theme,
    ) {
        let tile = t.tile_size;
        let inner = (tile - 8.0).max(16.0);
        let out = egui::ScrollArea::vertical()
            .max_height(t.height - 110.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                // Explicit columns from available width. NOTE: `horizontal_wrapped`
                // does not wrap `Frame` tiles (they size unbounded and run off the
                // right edge, clipped and unreachable), so a Grid is required.
                let outer = tile + 8.0; // tile + Frame inner_margin on both sides
                let cols = ((ui.available_width() + t.item_spacing) / (outer + t.item_spacing))
                    .floor() as usize;
                let cols = cols.max(1);
                self.last_columns = cols;
                // Center the whole grid block in the modal: a bare Grid
                // left-aligns, leaving the slack on the right.
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    egui::Grid::new("quick-modals-tiles")
                        .num_columns(cols)
                        .spacing([t.item_spacing, t.item_spacing])
                        .show(ui, |ui| {
                            for (pos, &opt_idx) in visible.iter().enumerate() {
                                let opt = self.modal.options[opt_idx].clone();
                                let selected = pos == self.cursor;
                                let tile_resp = egui::Frame::new()
                                    .fill(if selected { t.selected } else { t.surface })
                                    .corner_radius(t.effective_tile_radius())
                                    // NOTE: selected and unselected tiles must
                                    // share the same stroke width (only the color
                                    // differs). A thicker selected border would
                                    // change the tile's outer size and shift the
                                    // neighboring tiles in the grid.
                                    .stroke(egui::Stroke::new(
                                        t.border_width,
                                        if selected { t.accent } else { t.border },
                                    ))
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
                                        if let Some(tex) =
                                            self.icon_texture(ctx, &opt.id, &opt.icon)
                                        {
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
                                    .response
                                    .on_hover_text(opt.label.clone());
                                // Keyboard navigation: center the selected tile.
                                if self.scroll_to_cursor && pos == self.cursor {
                                    tile_resp.scroll_to_me(Some(egui::Align::Center));
                                }
                                // Hover takes the cursor only on fresh mouse movement,
                                // so it never fights keyboard navigation.
                                if tile_resp.hovered() && self.hover_fresh(ctx) {
                                    self.cursor = pos;
                                }
                                if tile_resp.clicked() {
                                    let sel = self.choose(visible, pos);
                                    self.finish(sel, ctx);
                                    return;
                                }
                                if (pos + 1) % cols == 0 || pos + 1 == visible.len() {
                                    ui.end_row();
                                }
                            }
                        });
                });
            });
        self.last_scroll_offset = out.state.offset;
        self.last_content_width = out.content_size.x;
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
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600.0, 500.0),
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

    /// …but a stationary pointer never overrides keyboard navigation.
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let t = self.modal.theme.clone();

        // Global keys.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.finish(None, ctx);
            return;
        }

        let visible = self.visible_indices();
        if self.cursor >= visible.len() {
            self.cursor = visible.len().saturating_sub(1);
        }

        // Grid: Up/Down move a full row; List: every arrow moves one item.
        let grid = matches!(self.modal.layout, Layout::Grid);
        let row_step = if grid { self.last_columns.max(1) } else { 1 };

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
            && !visible.is_empty()
        {
            self.cursor = if grid {
                (self.cursor + row_step).min(visible.len() - 1)
            } else {
                (self.cursor + 1) % visible.len()
            };
            self.scroll_to_cursor = self.modal.center_on_navigate;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight))
            && !visible.is_empty()
        {
            self.cursor = (self.cursor + 1) % visible.len();
            self.scroll_to_cursor = self.modal.center_on_navigate;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
            && !visible.is_empty()
        {
            self.cursor = if grid {
                self.cursor.saturating_sub(row_step)
            } else {
                self.cursor.checked_sub(1).unwrap_or(visible.len() - 1)
            };
            self.scroll_to_cursor = self.modal.center_on_navigate;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft))
            && !visible.is_empty()
        {
            self.cursor = self.cursor.checked_sub(1).unwrap_or(visible.len() - 1);
            self.scroll_to_cursor = self.modal.center_on_navigate;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            let sel = self.choose(&visible, self.cursor);
            self.finish(sel, ctx);
            return;
        }

        // Styling.
        let mut style = (*ctx.style()).clone();
        style.visuals.window_fill = t.background;
        style.visuals.panel_fill = t.background;
        style.visuals.selection.bg_fill = t.selected;
        style.visuals.widgets.inactive.bg_fill = t.surface;
        style.visuals.widgets.hovered.bg_fill = t.selected.linear_multiply(0.85);
        style.visuals.widgets.active.bg_fill = t.selected;
        ctx.set_style(style);

        let frame = egui::Frame::new()
            .fill(t.background)
            .corner_radius(t.corner_radius)
            .stroke(egui::Stroke::new(t.border_width, t.border))
            .inner_margin(t.padding)
            .outer_margin(t.margin);

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            ui.style_mut().override_text_style = Some(egui::TextStyle::Body);
            // Search field.
            if self.modal.search.enabled {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .id(self.search_id)
                        .hint_text(&self.modal.search.placeholder)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Vec2::new(10.0, 8.0)),
                );
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

            let visible = self.visible_indices();
            if visible.is_empty() {
                ui.colored_label(t.dim_text, "No matches");
                return;
            }

            if matches!(self.modal.layout, Layout::Grid) {
                self.show_grid(ctx, ui, &visible, &t);
            } else {
                self.show_list(ctx, ui, &visible, &t);
            }
        });

        // Request consumed: only keyboard moves re-center.
        self.scroll_to_cursor = false;

        // Shrink the window width to the measured content (never below
        // 320px, never above the theme width). The measurement must stay
        // put for consecutive updates first: the first multi-pass layouts
        // report transient widths that must never resize the window.
        // Skipped until the first real measurement so it never flashes narrow.
        if self.modal.fit_width_to_content
            && self.last_content_width > 0.0
            && !self.visible_indices().is_empty()
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
                    self.last_sent_width = want;
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        want, t.height,
                    )));
                }
            }
        } else {
            self.fit_stable_frames = 0;
        }
        self.last_hover_pos = ctx.input(|i| i.pointer.hover_pos());

        // Keep repainting for focus / texture readiness.
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}
