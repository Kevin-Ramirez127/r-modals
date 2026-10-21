//! Unit tests for [`super`].

use super::*;

/// Test-only icon (a 32×32 PNG) under `tests/assets/`, so unit tests never
/// depend on the example/demo assets.
const TEST_ICON: &str = "tests/assets/icon.png";

#[test]
fn decode_valid_png() {
    let bytes = include_bytes!("../../tests/assets/icon.png");
    assert!(decode_bytes(bytes).is_some());
    let img = decode_bytes(bytes).unwrap();
    assert!(img.width() > 0 && img.height() > 0);
}

#[test]
fn decode_invalid_bytes_returns_none() {
    assert!(decode_bytes(b"not an image").is_none());
    assert!(decode_bytes(include_bytes!("../../README.md")).is_none());
}

#[test]
fn load_icon_variants() {
    // File (ships with the tests, under tests/assets)
    assert!(load_icon(&IconSource::File(TEST_ICON.into())).is_some());
    // Missing file -> None (graceful)
    assert!(load_icon(&IconSource::File("tests/assets/does-not-exist.png".into())).is_none());
    // Bytes
    let bytes = include_bytes!("../../tests/assets/icon.png").to_vec();
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

fn modified_key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn viewport_closed(out: &egui::FullOutput) -> bool {
    out.viewport_output
        .get(&egui::ViewportId::ROOT)
        .is_some_and(|vo| {
            vo.commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Close))
        })
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

/// Once the search field has text, Left/Right/Home/End move its caret instead
/// of moving the row cursor. Empty-query list navigation remains covered by
/// `submenu_list_arrows_navigate`.
#[test]
fn text_field_arrows_edit_the_query() {
    use eframe::App as _;
    let (ctx, mut app, mut frame, _result) = prefix_app("");
    let mut t = 0.0;
    for _ in 0..3 {
        t += 1.0 / 60.0;
        let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
    }
    assert!(ctx.memory(|m| m.has_focus(app.search_id)));

    t += 1.0 / 60.0;
    let mut inp = headless_input(t);
    inp.events.push(egui::Event::Text("Alpha".to_string()));
    let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
    assert_eq!(app.query, "Alpha");

    let caret = |ctx: &egui::Context, app: &ModalApp| {
        egui::TextEdit::load_state(ctx, app.search_id)
            .and_then(|state| state.cursor.char_range())
            .map(|range| range.primary.index)
            .expect("text edit cursor")
    };
    assert_eq!(caret(&ctx, &app), 5);
    let row = app.cursor;

    for (key, expected) in [
        (egui::Key::ArrowLeft, 4),
        (egui::Key::Home, 0),
        (egui::Key::End, 5),
        (egui::Key::ArrowLeft, 4),
        (egui::Key::ArrowRight, 5),
    ] {
        t += 1.0 / 60.0;
        let mut inp = headless_input(t);
        inp.events.push(key_event(key));
        let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
        assert_eq!(caret(&ctx, &app), expected, "caret after {key:?}");
        assert_eq!(app.cursor, row, "row cursor moved after {key:?}");
        assert_eq!(app.query, "Alpha");
    }
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

fn shortcut_app(
    toml: &str,
) -> (
    egui::Context,
    ModalApp,
    eframe::Frame,
    Arc<Mutex<Option<ModalSelection>>>,
) {
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut modal = crate::Modal::new("shortcuts")
        .option(crate::ModalOption::new("a", "Alpha"))
        .option(crate::ModalOption::new("b", "Beta"))
        .shortcuts_toml(toml);
    // `Modal::run` resolves shortcuts; the headless harness calls it here.
    modal.resolve_shortcuts().unwrap();
    let result = Arc::new(Mutex::new(None));
    let app = ModalApp::new(&cc, modal, result.clone());
    let frame = eframe::Frame::_new_kittest();
    (ctx, app, frame, result)
}

/// A bound `option` shortcut picks that option, and its binding shows up
/// as the row hint.
#[test]
fn shortcut_key_picks_bound_option() {
    use eframe::App as _;
    let (ctx, mut app, mut frame, result) =
        shortcut_app("[[shortcut]]\nkey = \"Ctrl+B\"\noption = \"b\"\n");
    let mut t = 0.0;
    for _ in 0..3 {
        t += 1.0 / 60.0;
        let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
    }
    t += 1.0 / 60.0;
    let mut inp = headless_input(t);
    inp.events
        .push(modified_key_event(egui::Key::B, egui::Modifiers::CTRL));
    let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
    assert_eq!(
        result.lock().unwrap().clone().map(|s| s.id),
        Some("b".to_string())
    );
    assert_eq!(app.modal.options[1].shortcut.as_deref(), Some("Ctrl+B"));
}

/// A `pick` shortcut selects the highlighted row and tags the selection.
#[test]
fn shortcut_pick_tags_the_highlighted_row() {
    use eframe::App as _;
    let (ctx, mut app, mut frame, result) =
        shortcut_app("[[shortcut]]\nkey = \"Shift+Enter\"\npick = \"username\"\n");
    // Highlight the second row first.
    app.cursor = 1;
    let mut t = 0.0;
    for _ in 0..3 {
        t += 1.0 / 60.0;
        let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
    }
    t += 1.0 / 60.0;
    let mut inp = headless_input(t);
    inp.events.push(modified_key_event(
        egui::Key::Enter,
        egui::Modifiers {
            shift: true,
            ..egui::Modifiers::NONE
        },
    ));
    let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
    let sel = result.lock().unwrap().clone().expect("a pick");
    assert_eq!(sel.id, "b");
    assert_eq!(sel.shortcut.as_deref(), Some("username"));
}

/// `consume_key` ignores extra shift/alt, so the richer combo must be
/// tested first or `Ctrl+S` would swallow `Ctrl+Shift+S`.
#[test]
fn shortcut_matches_most_specific_combo() {
    use eframe::App as _;
    let (ctx, mut app, mut frame, result) = shortcut_app(
        "[[shortcut]]\nkey = \"Ctrl+S\"\noption = \"a\"\n\n\
         [[shortcut]]\nkey = \"Ctrl+Shift+S\"\noption = \"b\"\n",
    );
    let mut t = 0.0;
    for _ in 0..3 {
        t += 1.0 / 60.0;
        let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
    }
    t += 1.0 / 60.0;
    let mut inp = headless_input(t);
    inp.events.push(modified_key_event(
        egui::Key::S,
        egui::Modifiers {
            ctrl: true,
            shift: true,
            ..egui::Modifiers::NONE
        },
    ));
    let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
    assert_eq!(
        result.lock().unwrap().clone().map(|s| s.id),
        Some("b".to_string())
    );
}

/// A standalone command/shell shortcut runs and closes the modal without
/// a pick.
#[test]
fn shortcut_shell_runs_and_closes_without_pick() {
    use eframe::App as _;
    let (ctx, mut app, mut frame, result) =
        shortcut_app("[[shortcut]]\nkey = \"F5\"\nshell = \"true\"\n");
    let mut t = 0.0;
    for _ in 0..3 {
        t += 1.0 / 60.0;
        let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
    }
    t += 1.0 / 60.0;
    let mut inp = headless_input(t);
    inp.events.push(key_event(egui::Key::F5));
    let out = ctx.run(inp, |ctx| app.update(ctx, &mut frame));
    assert!(viewport_closed(&out), "shortcut should close the modal");
    assert!(result.lock().unwrap().is_none());
}

fn prefix_app(
    toml: &str,
) -> (
    egui::Context,
    ModalApp,
    eframe::Frame,
    Arc<Mutex<Option<ModalSelection>>>,
) {
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut modal = crate::Modal::new("prefixes")
        .option(crate::ModalOption::new("a", "Alpha"))
        .option(crate::ModalOption::new("b", "Beta"))
        .prefixes_toml(toml);
    // `Modal::run` resolves prefixes; the headless harness calls it here.
    modal.resolve_prefixes().unwrap();
    let result = Arc::new(Mutex::new(None));
    let app = ModalApp::new(&cc, modal, result.clone());
    let frame = eframe::Frame::_new_kittest();
    (ctx, app, frame, result)
}

/// A typed prefix confirmed with Enter runs its action and closes without a
/// pick, even though matching options are visible.
#[test]
fn prefix_enter_runs_action_and_closes_without_pick() {
    use eframe::App as _;
    let (ctx, mut app, mut frame, result) =
        prefix_app("[[prefix]]\nprefix = \">\"\nshell = \"true\"\n");
    let mut t = 0.0;
    for _ in 0..3 {
        t += 1.0 / 60.0;
        let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
    }
    // Prefix plus a remainder; the option list still contains a valid pick.
    app.query = "> hello".to_string();
    t += 1.0 / 60.0;
    let out = ctx.run(enter_event(t), |ctx| app.update(ctx, &mut frame));
    assert!(viewport_closed(&out), "prefix should close the modal");
    assert!(result.lock().unwrap().is_none());
}

/// Without a matching prefix, Enter keeps its normal meaning (pick a row).
#[test]
fn enter_without_prefix_still_picks() {
    use eframe::App as _;
    let (ctx, mut app, mut frame, result) =
        prefix_app("[[prefix]]\nprefix = \">\"\nshell = \"true\"\n");
    let mut t = 0.0;
    for _ in 0..3 {
        t += 1.0 / 60.0;
        let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
    }
    t += 1.0 / 60.0;
    let _ = ctx.run(enter_event(t), |ctx| app.update(ctx, &mut frame));
    assert_eq!(
        result.lock().unwrap().clone().map(|s| s.id),
        Some("a".to_string())
    );
}

/// `run_prefix` hands the trimmed remainder to the action as the selection's
/// query (and the prefix hint as the label).
#[test]
fn run_prefix_builds_selection_with_remainder() {
    let cp = crate::prefix::CompiledPrefix {
        prefix: ">".to_string(),
        hint: "Run".to_string(),
        action: crate::SelectAction::Callback(std::sync::Arc::new(|_| {})),
    };
    let sel = ModalApp::run_prefix(&cp, "ls -la", &[1, 2]);
    assert_eq!(sel.id, ">");
    assert_eq!(sel.label, "Run");
    assert_eq!(sel.query, "ls -la");
    assert_eq!(sel.path, vec![1, 2]);
    assert!(sel.custom_input);
}

/// An `=` after the prefix delimits the parameter handed to the action.
#[test]
fn prefix_equals_parameter_reaches_the_action() {
    use eframe::App as _;
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let seen = Arc::new(Mutex::new(None));
    let slot = seen.clone();
    let mut modal = crate::Modal::new("prefixes")
        .option(crate::ModalOption::new("a", "Alpha"))
        .prefix_action(
            ">search",
            crate::SelectAction::Callback(Arc::new(move |sel| {
                *slot.lock().unwrap() = Some(sel.query.clone());
            })),
        );
    modal.resolve_prefixes().unwrap();
    let result = Arc::new(Mutex::new(None));
    let mut app = ModalApp::new(&cc, modal, result.clone());
    let mut frame = eframe::Frame::_new_kittest();

    let mut t = 0.0;
    for _ in 0..3 {
        t += 1.0 / 60.0;
        let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
    }
    app.query = ">search=Hello World".to_string();
    t += 1.0 / 60.0;
    let out = ctx.run(enter_event(t), |ctx| app.update(ctx, &mut frame));

    assert!(viewport_closed(&out), "prefix should close the modal");
    assert!(result.lock().unwrap().is_none());
    assert_eq!(seen.lock().unwrap().as_deref(), Some("Hello World"));
}

#[test]
fn streaming_options_fill_the_loading_modal() {
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let result = Arc::new(Mutex::new(None));
    let (tx, rx) = std::sync::mpsc::channel();
    let modal = Modal::new("Search").loading_message("Loading…");
    let mut app = ModalApp::new_with_stream(&cc, modal, result, Some(rx), "Loading…".into());

    assert!(app.loading);
    assert!(app.modal.options.is_empty());

    tx.send(vec![
        ModalOption::new("a", "Alpha"),
        ModalOption::new("b", "Beta"),
    ])
    .unwrap();
    assert!(!app.poll_options(&ctx));
    assert!(!app.loading);
    assert_eq!(app.modal.options.len(), 2);
    assert_eq!(app.modal.options[0].label, "Alpha");
}

#[test]
fn streaming_disconnect_while_loading_cancels() {
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let result = Arc::new(Mutex::new(None));
    let (tx, rx) = std::sync::mpsc::channel::<Vec<ModalOption>>();
    let modal = Modal::new("Search").loading_message("Loading…");
    let mut app = ModalApp::new_with_stream(&cc, modal, result, Some(rx), "Loading…".into());

    drop(tx);
    assert!(app.poll_options(&ctx));
    assert!(!app.loading);
}

#[test]
fn empty_search_field_hides_the_empty_message() {
    let (_, mut app, _) = grid_app(0, true);
    app.query.clear();
    assert!(!app.shows_empty_message());
    app.query = "   ".to_string();
    assert!(!app.shows_empty_message());

    app.query = "ab".to_string();
    assert!(app.shows_empty_message());
}

/// With no partial prefix, Tab completes the focused row's label.
#[test]
fn tab_completes_the_focused_item() {
    use eframe::App as _;
    let (ctx, mut app, mut frame, _result) = prefix_app("");
    let mut t = 0.0;
    for _ in 0..3 {
        t += 1.0 / 60.0;
        let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
    }

    app.query = "Al".to_string();
    t += 1.0 / 60.0;
    let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));

    t += 1.0 / 60.0;
    let mut inp = headless_input(t);
    inp.events.push(key_event(egui::Key::Tab));
    let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));

    assert_eq!(app.query, "Alpha");
    let state = egui::TextEdit::load_state(&ctx, app.search_id).expect("text edit state");
    let range = state.cursor.char_range().expect("cursor range");
    assert_eq!(range.primary.index, app.query.chars().count());
    assert_eq!(range.secondary.index, app.query.chars().count());
}

/// Tab completion prefers a partial prefix over the focused row.
#[test]
fn tab_prefers_a_prefix_completion_over_the_item() {
    use eframe::App as _;
    let (ctx, mut app, mut frame, _result) = prefix_app(
        "[[prefix]]\nprefix = \">s\"\nshell = \"true\"\n         [[prefix]]\nprefix = \">sync\"\nshell = \"true\"\n",
    );
    let mut t = 0.0;
    for _ in 0..3 {
        t += 1.0 / 60.0;
        let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
    }

    app.query = ">s".to_string();
    t += 1.0 / 60.0;
    let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
    t += 1.0 / 60.0;
    let mut inp = headless_input(t);
    inp.events.push(key_event(egui::Key::Tab));
    let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));

    assert_eq!(app.query, ">sync");
}

/// Tab completes a partially typed prefix and leaves the cursor at the end.
#[test]
fn tab_completes_the_closest_prefix() {
    use eframe::App as _;
    let (ctx, mut app, mut frame, _result) = prefix_app(
        "[[prefix]]\nprefix = \">sync\"\nshell = \"true\"\n\
         [[prefix]]\nprefix = \">show items\"\nshell = \"true\"\n",
    );
    let mut t = 0.0;
    for _ in 0..3 {
        t += 1.0 / 60.0;
        let _ = ctx.run(headless_input(t), |ctx| app.update(ctx, &mut frame));
    }

    app.query = ">sh".to_string();
    t += 1.0 / 60.0;
    let mut inp = headless_input(t);
    inp.events.push(key_event(egui::Key::Tab));
    let _ = ctx.run(inp, |ctx| app.update(ctx, &mut frame));

    assert_eq!(app.query, ">show items");
    let state = egui::TextEdit::load_state(&ctx, app.search_id).expect("text edit state");
    let range = state.cursor.char_range().expect("cursor range");
    assert_eq!(range.primary.index, app.query.chars().count());
    assert_eq!(range.secondary.index, app.query.chars().count());
}
