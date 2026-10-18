//! Unit tests for [`super`].

use super::*;

fn themed(css: &str) -> Theme {
    Theme::from_css(css).expect("valid css")
}

#[test]
fn empty_css_keeps_defaults() {
    let t = Theme::from_css("").unwrap();
    assert_eq!(t.width, Theme::dark().width);
    assert_eq!(t.background, Theme::dark().background);
}

#[test]
fn full_modal_mapping() {
    let t = themed(
        r#"modal {
            width: 600px; height: 400px;
            background: #18181c; color: rgb(235, 235, 240);
            border-radius: 16px; padding: 10px; margin: 6px;
            font-size: 14px; gap: 6px;
            accent-color: #6490dc;
        }"#,
    );
    assert_eq!(t.width, 600.0);
    assert_eq!(t.height, 400.0);
    assert_eq!(t.background, egui::Color32::from_rgb(24, 24, 28));
    assert_eq!(t.text, egui::Color32::from_rgb(235, 235, 240));
    assert_eq!(t.corner_radius, 16.0);
    assert_eq!(t.padding, 10.0);
    assert_eq!(t.margin, 6.0);
    assert_eq!(t.font_size, 14.0);
    assert_eq!(t.item_spacing, 6.0);
    assert_eq!(t.accent, egui::Color32::from_rgb(100, 144, 220));
}

#[test]
fn tiles_and_description() {
    let t = themed(
        r#"
        tile { background: #222228; border-radius: 8px; width: 128px; height: 128px; }
        tile icon { width: 32px; height: 32px; }
        tile[selected] { background: rgb(64, 110, 210); }
        tile description { color: rebeccapurple; }
        "#,
    );
    assert_eq!(t.surface, egui::Color32::from_rgb(34, 34, 40));
    assert_eq!(t.tile_radius, Some(8.0));
    assert_eq!(t.tile_size, 128.0);
    assert_eq!(t.icon_size, 32.0);
    assert_eq!(t.selected, egui::Color32::from_rgb(64, 110, 210));
    assert_eq!(t.dim_text, egui::Color32::from_rgb(102, 51, 153));
    assert!(t.show_descriptions);
}

#[test]
fn single_tile_dimension_is_enough() {
    let t = themed("tile { width: 96px; }");
    assert_eq!(t.tile_size, 96.0);
}

#[test]
fn search_field_mapping() {
    let t = themed(
        r#"
        search {
            background: #222228; color: #f0f0f5; font-size: 18px;
            border-radius: 8px;
        }
        search hint { color: #888895; }
        "#,
    );
    assert_eq!(t.search_background, egui::Color32::from_rgb(34, 34, 40));
    assert_eq!(t.search_text, egui::Color32::from_rgb(240, 240, 245));
    assert_eq!(t.search_font_size, Some(18.0));
    assert_eq!(t.effective_search_font_size(), 18.0);
    assert_eq!(t.search_radius, Some(8.0));
    assert_eq!(t.effective_search_radius(), 8.0);
    assert_eq!(t.search_hint, egui::Color32::from_rgb(136, 136, 149));
}

#[test]
fn search_defaults_follow_modal() {
    let t = themed("modal { font-size: 20px; border-radius: 10px; }");
    assert_eq!(t.search_font_size, None);
    assert_eq!(t.effective_search_font_size(), 20.0);
    assert_eq!(t.search_radius, None);
    assert_eq!(t.effective_search_radius(), 6.0);
}

#[test]
fn borders_are_rejected_always_borderless() {
    for bad in [
        "modal { border: none; }",
        "modal { border: 1px solid red; }",
        "modal { border-width: thin; }",
        "modal { border-color: red; }",
        "search { border: none; }",
        "search { border-width: 1px; }",
        "search { border-color: red; }",
    ] {
        assert!(Theme::from_css(bad).is_err(), "expected error for {bad:?}");
    }
}

#[test]
fn display_none_hides_descriptions() {
    let t = themed("tile description { display: none; color: red; }");
    assert!(!t.show_descriptions);
    assert_eq!(t.dim_text, egui::Color32::from_rgb(255, 0, 0));
}

#[test]
fn mismatched_square_sizes_are_rejected() {
    for bad in [
        "tile { width: 100px; height: 200px; }",
        "tile icon { width: 10px; height: 20px; }",
        "tile description { display: block; }",
        "tile icon { background: red; }",
    ] {
        assert!(Theme::from_css(bad).is_err(), "expected error for {bad:?}");
    }
}

#[test]
fn last_rule_wins() {
    let t = themed("modal { color: red; } modal { color: blue; }");
    assert_eq!(t.text, egui::Color32::from_rgb(0, 0, 255));
}

#[test]
fn important_beats_later_normal_rule() {
    let t = themed("modal { color: red !important; } modal { color: blue; }");
    assert_eq!(t.text, egui::Color32::from_rgb(255, 0, 0));
}

#[test]
fn width_auto_keeps_current() {
    let t = themed("modal { width: auto; }");
    assert_eq!(t.width, Theme::dark().width);
}

#[test]
fn transparent_is_rgba_zero() {
    let t = themed("tile { background: transparent; }");
    assert_eq!(
        t.surface,
        egui::Color32::from_rgba_premultiplied(0, 0, 0, 0)
    );
}

#[test]
fn broken_rules_are_dropped_not_fatal() {
    // Browser-style recovery: an unclosed block applies nothing.
    let t = Theme::from_css("modal {").unwrap();
    assert_eq!(t.width, Theme::dark().width);
}

#[test]
fn rejects_everything_outside_the_subset() {
    for bad in [
        "modal { width: 10px; color }",          // truncated declaration
        ".tile { color: red; }",                 // class selector
        "button { color: red; }",                // unknown type
        "tile:hover { color: red; }",            // pseudo-class
        "modal { display: flex; }",              // unknown property
        "tile { color: red; }",                  // wrong target
        "tile description { background: red; }", // wrong target
        "search { width: 10px; }",               // search has no box size
        "search { gap: 4px; }",                  // wrong target
        "search hint { background: red; }",      // wrong target
        "modal { width: 10em; }",                // non-px unit
        "modal { width: 50%; }",                 // percentage
        "modal { font-size: medium; }",          // keyword size
        "modal { padding: 1px 2px; }",           // non-uniform
        "modal { border-radius: 1px 2px; }",     // non-uniform
        "modal { border-radius: 8px / 4px; }",   // elliptical
        "modal { margin: 0 auto; }",             // auto in rect
        "modal { color: currentcolor; }",        // unresolvable color
        "modal { background: url(x.png); }",     // image background
        "@media (min-width: 1px) { modal { color: red; } }", // at-rule
        "modal { &:hover { color: red; } }",     // nested rule
    ] {
        assert!(Theme::from_css(bad).is_err(), "expected error for {bad:?}");
    }
}
