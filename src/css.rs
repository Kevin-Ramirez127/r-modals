//! CSS stylesheets for [`Theme`](crate::Theme), parsed with `lightningcss`.
//!
//! Supported selectors (anything else is an error):
//!
//! - `modal` — the window: `width`, `height`, `background`/`background-color`,
//!   `color`, `border-radius`, `padding`, `margin`, `font-size`,
//!   `border`/`border-width`/`border-color`, `gap`, `accent-color`
//! - `tile` — option rows/tiles: `background`/`background-color`,
//!   `border-radius`, `width`/`height` (must match: tiles are square)
//! - `tile icon` — the icon inside list rows: `width`/`height` (must match)
//! - `tile[selected]` — the selected row/tile: `background`/`background-color`
//! - `tile description` — secondary text: `color`, `display: none` (hides it)
//!
//! Only `px` lengths are supported (`0` counts as `0px`). Anything else —
//! `em`, `%`, `auto` where a length is required — is an error, except
//! `width`/`height`/`accent-color`, where `auto` keeps the current value.
//! Rect shorthands (`padding`, `margin`, `border-width`, `border-color`,
//! `border-radius`) must be uniform: write a single value. Later rules win
//! (no specificity, `!important` always wins).

use crate::Theme;
use lightningcss::{
    properties::{
        border::LineStyle,
        display::{Display, DisplayKeyword},
        ui::ColorOrAuto,
        Property,
    },
    rules::CssRule,
    stylesheet::{ParserOptions, StyleSheet},
    traits::ToCss,
    values::{
        color::CssColor,
        length::{LengthPercentage, LengthPercentageOrAuto, LengthValue},
        size::Size2D,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Modal,
    Tile,
    TileIcon,
    TileSelected,
    TileDescription,
}

fn target_of(selector: &str) -> Result<Target, String> {
    match selector.trim().to_ascii_lowercase().as_str() {
        "modal" => Ok(Target::Modal),
        "tile" => Ok(Target::Tile),
        "tile icon" => Ok(Target::TileIcon),
        "tile[selected]" => Ok(Target::TileSelected),
        "tile description" => Ok(Target::TileDescription),
        other => Err(format!(
            "unsupported selector {other:?}; supported: modal, tile, `tile icon`, tile[selected], `tile description`"
        )),
    }
}

/// Apply a CSS stylesheet on top of `theme` (see module docs for the subset).
///
/// Malformed input the parser can recover from (e.g. an unclosed block) is
/// dropped browser-style; anything that parses but falls outside the
/// supported subset is a hard error.
pub fn apply_css(theme: &mut Theme, css: &str) -> Result<(), String> {
    let sheet = StyleSheet::parse(css, ParserOptions::default())
        .map_err(|e| format!("CSS parse error: {e:?}"))?;
    // Normal declarations first across all rules, `!important` after:
    // important wins globally, later rules win within each pass.
    for important in [false, true] {
        for rule in &sheet.rules.0 {
            let CssRule::Style(style) = rule else {
                return Err("only plain style rules are supported (no @media, @import, …)".into());
            };
            if !style.rules.0.is_empty() {
                return Err("nested rules are not supported".into());
            }
            let decls = if important {
                &style.declarations.important_declarations
            } else {
                &style.declarations.declarations
            };
            for selector in style.selectors.0.iter() {
                let text = selector
                    .to_css_string(Default::default())
                    .map_err(|e| format!("CSS error: {e:?}"))?;
                let target = target_of(&text)?;
                // Square sizes first (tile/icon width+height must match).
                apply_square_size(theme, target, decls)?;
                for decl in decls {
                    if is_square_size(target, decl) {
                        continue;
                    }
                    apply_declaration(theme, target, decl)?;
                }
            }
        }
    }
    Ok(())
}

fn color(c: &CssColor, what: &str) -> Result<egui::Color32, String> {
    match c {
        CssColor::RGBA(rgba) => Ok(egui::Color32::from_rgba_premultiplied(
            rgba.red, rgba.green, rgba.blue, rgba.alpha,
        )),
        _ => Err(format!(
            "{what}: only plain colors are supported (no currentcolor, …)"
        )),
    }
}

fn px(length: &LengthPercentage, what: &str) -> Result<f32, String> {
    match length {
        LengthPercentage::Dimension(LengthValue::Px(v)) => Ok(*v),
        _ => Err(format!("{what}: only px lengths are supported")),
    }
}

fn uniform(top: f32, right: f32, bottom: f32, left: f32, what: &str) -> Result<f32, String> {
    if top == right && top == bottom && top == left {
        Ok(top)
    } else {
        Err(format!(
            "{what}: only uniform values are supported, write a single value"
        ))
    }
}

fn rect_px(
    top: &LengthPercentageOrAuto,
    right: &LengthPercentageOrAuto,
    bottom: &LengthPercentageOrAuto,
    left: &LengthPercentageOrAuto,
    what: &str,
) -> Result<f32, String> {
    fn side(v: &LengthPercentageOrAuto, what: &str) -> Result<f32, String> {
        match v {
            LengthPercentageOrAuto::LengthPercentage(lp) => px(lp, what),
            LengthPercentageOrAuto::Auto => Err(format!("{what}: auto is not supported here")),
        }
    }
    uniform(
        side(top, what)?,
        side(right, what)?,
        side(bottom, what)?,
        side(left, what)?,
        what,
    )
}

fn border_side_width(
    w: &lightningcss::properties::border::BorderSideWidth,
    what: &str,
) -> Result<f32, String> {
    use lightningcss::properties::border::BorderSideWidth;
    use lightningcss::values::length::Length;
    match w {
        BorderSideWidth::Length(Length::Value(LengthValue::Px(v))) => Ok(*v),
        // CSS spec keywords.
        BorderSideWidth::Thin => Ok(1.0),
        BorderSideWidth::Medium => Ok(3.0),
        BorderSideWidth::Thick => Ok(5.0),
        _ => Err(format!("{what}: only px border widths are supported")),
    }
}

/// `gap` fields are private, so read the row value back from its text.
fn gap_px(gap: &lightningcss::properties::align::Gap, what: &str) -> Result<f32, String> {
    let text = gap
        .to_css_string(Default::default())
        .map_err(|e| format!("{what}: {e:?}"))?;
    let first = text.split_whitespace().next().unwrap_or("");
    if first == "0" {
        return Ok(0.0);
    }
    first
        .strip_suffix("px")
        .and_then(|n| n.parse::<f32>().ok())
        .ok_or_else(|| format!("{what}: only px gaps are supported"))
}

fn apply_declaration(theme: &mut Theme, target: Target, prop: &Property) -> Result<(), String> {
    match target {
        Target::Modal => match prop {
            Property::Width(s) => {
                theme.width = size_px(s, "modal width")?.unwrap_or(theme.width);
            }
            Property::Height(s) => {
                theme.height = size_px(s, "modal height")?.unwrap_or(theme.height);
            }
            Property::BackgroundColor(c) => theme.background = color(c, "modal background")?,
            Property::Background(bgs) => {
                let [bg] = bgs.as_slice() else {
                    return Err("modal background: multiple backgrounds are not supported".into());
                };
                if background_image_present(bg, "modal background")? {
                    return Err("modal background: background images are not supported".into());
                }
                theme.background = color(&bg.color, "modal background")?;
            }
            Property::Color(c) => theme.text = color(c, "modal color")?,
            Property::AccentColor(c) => match c {
                ColorOrAuto::Auto => {}
                ColorOrAuto::Color(c) => theme.accent = color(c, "modal accent-color")?,
            },
            Property::BorderRadius(r, _) => {
                theme.corner_radius = uniform_radius(r, "modal border-radius")?;
            }
            Property::Padding(p) => {
                theme.padding = rect_px(&p.top, &p.right, &p.bottom, &p.left, "modal padding")?;
            }
            Property::Margin(m) => {
                theme.margin = rect_px(&m.top, &m.right, &m.bottom, &m.left, "modal margin")?;
            }
            Property::FontSize(s) => {
                theme.font_size = font_px(s, "modal font-size")?;
            }
            Property::Border(b) => {
                if matches!(b.style, LineStyle::None) {
                    theme.border_width = 0.0;
                } else {
                    theme.border_width = border_side_width(&b.width, "modal border")?;
                    theme.border = color(&b.color, "modal border")?;
                }
            }
            Property::BorderWidth(w) => {
                theme.border_width = uniform(
                    border_side_width(&w.top, "modal border-width")?,
                    border_side_width(&w.right, "modal border-width")?,
                    border_side_width(&w.bottom, "modal border-width")?,
                    border_side_width(&w.left, "modal border-width")?,
                    "modal border-width",
                )?;
            }
            Property::BorderColor(c) => {
                theme.border =
                    uniform_color(&c.top, &c.right, &c.bottom, &c.left, "modal border-color")?;
            }
            Property::Gap(g) => theme.item_spacing = gap_px(g, "modal gap")?,
            Property::RowGap(v) | Property::ColumnGap(v) => {
                theme.item_spacing = gap_value_px(v, "modal gap")?;
            }
            _ => return Err(unsupported(target, prop)),
        },
        Target::Tile => match prop {
            Property::BackgroundColor(c) => theme.surface = color(c, "tile background")?,
            Property::Background(bgs) => {
                let [bg] = bgs.as_slice() else {
                    return Err("tile background: multiple backgrounds are not supported".into());
                };
                if background_image_present(bg, "tile background")? {
                    return Err("tile background: background images are not supported".into());
                }
                theme.surface = color(&bg.color, "tile background")?;
            }
            Property::BorderRadius(r, _) => {
                theme.tile_radius = Some(uniform_radius(r, "tile border-radius")?);
            }
            _ => return Err(unsupported(target, prop)),
        },
        // Square sizes (`width`/`height`) are applied by `apply_square_size`
        // before this runs; anything else on an icon is rejected.
        Target::TileIcon => return Err(unsupported(target, prop)),
        Target::TileSelected => match prop {
            Property::BackgroundColor(c) => theme.selected = color(c, "tile[selected] background")?,
            Property::Background(bgs) => {
                let [bg] = bgs.as_slice() else {
                    return Err(
                        "tile[selected] background: multiple backgrounds are not supported".into(),
                    );
                };
                if background_image_present(bg, "tile[selected] background")? {
                    return Err(
                        "tile[selected] background: background images are not supported".into(),
                    );
                }
                theme.selected = color(&bg.color, "tile[selected] background")?;
            }
            _ => return Err(unsupported(target, prop)),
        },
        Target::TileDescription => match prop {
            Property::Color(c) => theme.dim_text = color(c, "tile description color")?,
            Property::Display(d) => match d {
                Display::Keyword(DisplayKeyword::None) => theme.show_descriptions = false,
                _ => {
                    return Err(
                        "tile description display: only `display: none` is supported".into(),
                    );
                }
            },
            _ => return Err(unsupported(target, prop)),
        },
    }
    Ok(())
}

fn unsupported(target: Target, prop: &Property) -> String {
    let name = match target {
        Target::Modal => "modal",
        Target::Tile => "tile",
        Target::TileIcon => "`tile icon`",
        Target::TileSelected => "tile[selected]",
        Target::TileDescription => "`tile description`",
    };
    match prop.to_css_string(false, Default::default()) {
        Ok(text) => format!("unsupported property for {name}: {text}"),
        Err(_) => format!("unsupported property for {name}"),
    }
}

/// The background `Image` enum is private, so detect anything but `none`
/// through its text form.
fn background_image_present(
    bg: &lightningcss::properties::background::Background,
    what: &str,
) -> Result<bool, String> {
    let text = bg
        .image
        .to_css_string(Default::default())
        .map_err(|e| format!("{what}: {e:?}"))?;
    Ok(text != "none")
}

/// `None` = `auto` (keep current); anything else must be px.
fn size_px(s: &lightningcss::properties::size::Size, what: &str) -> Result<Option<f32>, String> {
    use lightningcss::properties::size::Size;
    match s {
        Size::Auto => Ok(None),
        Size::LengthPercentage(lp) => Ok(Some(px(lp, what)?)),
        _ => Err(format!("{what}: only px lengths or auto are supported")),
    }
}

/// Whether this declaration is a square size handled by `apply_square_size`.
fn is_square_size(target: Target, decl: &Property) -> bool {
    matches!(target, Target::Tile | Target::TileIcon)
        && matches!(decl, Property::Width(_) | Property::Height(_))
}

/// Tiles and icons are square: `width`/`height` must match when both are
/// given (a single dimension is enough). Last one wins per pass.
fn apply_square_size(theme: &mut Theme, target: Target, decls: &[Property]) -> Result<(), String> {
    let (name, set): (&str, fn(&mut Theme, f32)) = match target {
        Target::Tile => ("tile", |t, v| t.tile_size = v),
        Target::TileIcon => ("tile icon", |t, v| t.icon_size = v),
        _ => return Ok(()),
    };
    let mut width = None;
    let mut height = None;
    for decl in decls {
        match decl {
            Property::Width(s) => width = size_px(s, &format!("{name} width"))?,
            Property::Height(s) => height = size_px(s, &format!("{name} height"))?,
            _ => {}
        }
    }
    match (width, height) {
        (Some(w), Some(h)) if w != h => Err(format!(
            "{name} width and height must match (tiles are square)"
        )),
        (Some(v), _) | (_, Some(v)) => {
            set(theme, v);
            Ok(())
        }
        (None, None) => Ok(()),
    }
}

fn font_px(s: &lightningcss::properties::font::FontSize, what: &str) -> Result<f32, String> {
    use lightningcss::properties::font::FontSize;
    match s {
        FontSize::Length(lp) => px(lp, what),
        _ => Err(format!("{what}: only px font sizes are supported")),
    }
}

fn gap_value_px(v: &lightningcss::properties::align::GapValue, what: &str) -> Result<f32, String> {
    use lightningcss::properties::align::GapValue;
    match v {
        GapValue::LengthPercentage(lp) => px(lp, what),
        _ => Err(format!("{what}: only px gaps are supported")),
    }
}

fn uniform_color(
    top: &CssColor,
    right: &CssColor,
    bottom: &CssColor,
    left: &CssColor,
    what: &str,
) -> Result<egui::Color32, String> {
    let (t, r, b, l) = (
        color(top, what)?,
        color(right, what)?,
        color(bottom, what)?,
        color(left, what)?,
    );
    if t == r && t == b && t == l {
        Ok(t)
    } else {
        Err(format!(
            "{what}: only uniform values are supported, write a single value"
        ))
    }
}

fn uniform_radius(
    r: &lightningcss::properties::border_radius::BorderRadius,
    what: &str,
) -> Result<f32, String> {
    let corners = [&r.top_left, &r.top_right, &r.bottom_right, &r.bottom_left];
    let mut out = None;
    for Size2D(x, y) in corners {
        let (x, y) = (px(x, what)?, px(y, what)?);
        if x != y {
            return Err(format!("{what}: elliptical radii are not supported"));
        }
        match out {
            None => out = Some(x),
            Some(v) if v == x => {}
            Some(_) => {
                return Err(format!(
                    "{what}: only uniform values are supported, write a single value"
                ));
            }
        }
    }
    Ok(out.unwrap_or(0.0))
}

#[cfg(test)]
mod tests {
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
                font-size: 14px; border: 2px solid #111111; gap: 6px;
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
        assert_eq!(t.border_width, 2.0);
        assert_eq!(t.border, egui::Color32::from_rgb(17, 17, 17));
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
    fn border_none_means_no_border() {
        let t = themed("modal { border: none; }");
        assert_eq!(t.border_width, 0.0);
    }

    #[test]
    fn border_thin_keyword() {
        let t = themed("modal { border-width: thin; border-color: red; }");
        assert_eq!(t.border_width, 1.0);
        assert_eq!(t.border, egui::Color32::from_rgb(255, 0, 0));
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
}
