//! Unit tests for [`super`].

use super::*;

fn dark_css() -> &'static str {
    include_str!("../../assets/themes/dark.css")
}

#[test]
fn margin_via_css() {
    let t = Theme::from_css("modal { margin: 0px; }").unwrap();
    assert_eq!(t.margin, 0.0);
    let t = Theme::from_css("modal { margin: 24px; }").unwrap();
    assert_eq!(t.margin, 24.0);
}

#[test]
fn presets_match_expected_values() {
    let dark = Theme::dark();
    assert_eq!(dark.background, Color32::from_rgb(24, 24, 28));
    assert_eq!(dark.surface, Color32::from_rgb(34, 34, 40));
    assert_eq!(dark.text, Color32::from_rgb(235, 235, 240));
    assert_eq!(dark.tile_radius, None);
    assert_eq!(dark.search_background, Color32::from_rgb(34, 34, 40));
    assert_eq!(dark.search_text, Color32::from_rgb(235, 235, 240));
    assert_eq!(dark.search_hint, Color32::from_rgb(150, 150, 165));
    assert_eq!(dark.search_radius, None);
    assert_eq!(dark.search_font_size, None);

    let light = Theme::light();
    assert_eq!(light.background, Color32::from_rgb(245, 245, 248));
    assert_eq!(light.surface, Color32::WHITE);
    assert_eq!(light.selected, Color32::from_rgb(50, 110, 230));
    assert_eq!(light.text, Color32::from_rgb(20, 20, 25));
    assert_eq!(light.dim_text, Color32::from_rgb(110, 110, 125));
    assert_eq!(light.accent, Color32::from_rgb(40, 90, 220));
    assert_eq!(light.width, 560.0);
    assert_eq!(light.height, 380.0);
    assert_eq!(light.corner_radius, 12.0);
    assert_eq!(light.padding, 12.0);
    assert_eq!(light.margin, 8.0);
    assert_eq!(light.item_spacing, 4.0);
    assert_eq!(light.font_size, 15.0);
    assert_eq!(light.search_background, Color32::WHITE);
    assert_eq!(light.search_text, Color32::from_rgb(20, 20, 25));
    assert_eq!(light.search_hint, Color32::from_rgb(110, 110, 125));
    assert_eq!(light.search_radius, None);
    assert_eq!(light.search_font_size, None);
    assert_eq!(light.icon_size, 28.0);
    assert_eq!(light.tile_size, 112.0);
    assert_eq!(light.tile_radius, None);
    assert!(light.show_descriptions);

    let td = Theme::transparent_dark();
    assert_eq!(
        td.background,
        Color32::from_rgba_premultiplied(20, 20, 26, 230)
    );
    assert_eq!(
        td.surface,
        Color32::from_rgba_premultiplied(35, 35, 45, 230)
    );
    // Transparent presets only change translucency.
    assert_eq!(td.text, dark.text);
    assert_eq!(td.width, dark.width);
}

#[test]
fn preset_stylesheets_fully_specify_their_theme() {
    // Each built-in file must carry every value that defines it:
    // layering it over a *different* base still yields the preset.
    let mut t = Theme::light();
    t.apply_css(dark_css()).unwrap();
    assert_eq!(t.background, Theme::dark().background);
    assert_eq!(t.surface, Theme::dark().surface);
    assert_eq!(t.selected, Theme::dark().selected);
    assert_eq!(t.text, Theme::dark().text);
    assert_eq!(t.dim_text, Theme::dark().dim_text);
    assert_eq!(t.accent, Theme::dark().accent);
    assert_eq!(t.width, Theme::dark().width);
    assert_eq!(t.height, Theme::dark().height);
    assert_eq!(t.corner_radius, Theme::dark().corner_radius);
    assert_eq!(t.padding, Theme::dark().padding);
    assert_eq!(t.margin, Theme::dark().margin);
    assert_eq!(t.item_spacing, Theme::dark().item_spacing);
    assert_eq!(t.font_size, Theme::dark().font_size);
    assert_eq!(t.search_background, Theme::dark().search_background);
    assert_eq!(t.search_text, Theme::dark().search_text);
    assert_eq!(t.search_hint, Theme::dark().search_hint);
    assert_eq!(t.search_radius, Theme::dark().search_radius);
    assert_eq!(t.search_font_size, Theme::dark().search_font_size);
    assert_eq!(t.icon_size, Theme::dark().icon_size);
    assert_eq!(t.tile_size, Theme::dark().tile_size);
    assert_eq!(t.tile_radius, Theme::dark().tile_radius);
    assert_eq!(t.show_descriptions, Theme::dark().show_descriptions);
}
