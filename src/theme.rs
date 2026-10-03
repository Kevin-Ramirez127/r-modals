use egui::Color32;

/// Where to place the modal window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    /// Center of the current monitor.
    #[default]
    Center,
    /// Top-centered with a vertical offset in logical pixels.
    TopCenter {
        /// Distance from the top edge.
        y_offset: u16,
    },
    /// Fixed outer position in logical pixels.
    Custom { x: u16, y: u16 },
}

/// How options are laid out in the modal window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    /// One row per option: thumbnail followed by label + description.
    #[default]
    List,
    /// Square tiles where each tile IS the image (no text).
    /// Label is shown as a hover tooltip; search still filters on it.
    Grid,
}

/// Visual customization for the modal.
///
/// Themes are styled with CSS stylesheets: [`Theme::from_css`],
/// [`Theme::apply_css`], or [`Modal::stylesheet`](crate::Modal::stylesheet).
#[derive(Debug, Clone)]
pub struct Theme {
    /// Window width in logical pixels.
    pub width: f32,
    /// Window height in logical pixels.
    pub height: f32,
    /// Icon size in logical pixels (list rows).
    pub icon_size: f32,
    /// Tile edge length in logical pixels (used with [`Layout::Grid`]).
    pub tile_size: f32,
    /// Corner radius of option rows/tiles. `None` (default) derives 60% of
    /// `corner_radius`, so tiles follow the modal shape automatically.
    pub tile_radius: Option<f32>,
    /// Corner rounding.
    pub corner_radius: f32,
    /// Outer padding.
    pub padding: f32,
    /// Outer margin around the frame (space to the window edge).
    pub margin: f32,
    /// Gap between rows.
    pub item_spacing: f32,
    /// Colors.
    pub background: Color32,
    pub surface: Color32,
    pub selected: Color32,
    pub text: Color32,
    pub dim_text: Color32,
    pub accent: Color32,
    pub border: Color32,
    /// Border width (0 = none).
    pub border_width: f32,
    /// Search field font size.
    pub font_size: f32,
    /// Show item descriptions.
    pub show_descriptions: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Unstyled base values (the dark defaults). Presets layer their
    /// embedded stylesheet over this.
    fn base() -> Self {
        Self {
            width: 560.0,
            height: 380.0,
            icon_size: 28.0,
            tile_size: 112.0,
            tile_radius: None,
            corner_radius: 12.0,
            padding: 12.0,
            margin: 8.0,
            item_spacing: 4.0,
            background: Color32::from_rgb(24, 24, 28),
            surface: Color32::from_rgb(34, 34, 40),
            selected: Color32::from_rgb(64, 110, 210),
            text: Color32::from_rgb(235, 235, 240),
            dim_text: Color32::from_rgb(150, 150, 165),
            accent: Color32::from_rgb(100, 160, 255),
            border: Color32::from_rgb(60, 60, 72),
            border_width: 1.0,
            font_size: 15.0,
            show_descriptions: true,
        }
    }

    /// Dark theme (default), defined by the embedded `dark.css` stylesheet.
    pub fn dark() -> Self {
        let mut theme = Self::base();
        theme
            .apply_css(include_str!("../assets/themes/dark.css"))
            .expect("built-in dark.css is valid");
        theme
    }

    /// Light theme, defined by the embedded `light.css` stylesheet.
    pub fn light() -> Self {
        let mut theme = Self::base();
        theme
            .apply_css(include_str!("../assets/themes/light.css"))
            .expect("built-in light.css is valid");
        theme
    }

    /// Transparent dark theme, defined by the embedded
    /// `transparent_dark.css` stylesheet.
    pub fn transparent_dark() -> Self {
        let mut theme = Self::base();
        theme
            .apply_css(include_str!("../assets/themes/transparent_dark.css"))
            .expect("built-in transparent_dark.css is valid");
        theme
    }

    /// Effective option corner radius: explicit override or 60% of the modal radius.
    pub(crate) fn effective_tile_radius(&self) -> f32 {
        self.tile_radius.unwrap_or(self.corner_radius * 0.6)
    }

    /// Build a theme from a CSS stylesheet (see `css` module docs for the
    /// supported selectors and properties). Starts from the dark defaults.
    ///
    /// ```no_run
    /// use quick_modals::Theme;
    /// let theme = Theme::from_css("modal { background: #18181c; border-radius: 12px; }").unwrap();
    /// ```
    pub fn from_css(css: &str) -> Result<Self, String> {
        let mut theme = Self::base();
        theme.apply_css(css)?;
        Ok(theme)
    }

    /// Layer a CSS stylesheet over this theme (last rule wins).
    pub fn apply_css(&mut self, css: &str) -> Result<(), String> {
        crate::css::apply_css(self, css)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dark_css() -> &'static str {
        include_str!("../assets/themes/dark.css")
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

        let light = Theme::light();
        assert_eq!(light.background, Color32::from_rgb(245, 245, 248));
        assert_eq!(light.surface, Color32::WHITE);
        assert_eq!(light.selected, Color32::from_rgb(50, 110, 230));
        assert_eq!(light.text, Color32::from_rgb(20, 20, 25));
        assert_eq!(light.dim_text, Color32::from_rgb(110, 110, 125));
        assert_eq!(light.accent, Color32::from_rgb(40, 90, 220));
        assert_eq!(light.border, Color32::from_rgb(210, 210, 220));
        assert_eq!(light.border_width, 1.0);
        assert_eq!(light.width, 560.0);
        assert_eq!(light.height, 380.0);
        assert_eq!(light.corner_radius, 12.0);
        assert_eq!(light.padding, 12.0);
        assert_eq!(light.margin, 8.0);
        assert_eq!(light.item_spacing, 4.0);
        assert_eq!(light.font_size, 15.0);
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
        assert_eq!(t.border, Theme::dark().border);
        assert_eq!(t.border_width, Theme::dark().border_width);
        assert_eq!(t.width, Theme::dark().width);
        assert_eq!(t.height, Theme::dark().height);
        assert_eq!(t.corner_radius, Theme::dark().corner_radius);
        assert_eq!(t.padding, Theme::dark().padding);
        assert_eq!(t.margin, Theme::dark().margin);
        assert_eq!(t.item_spacing, Theme::dark().item_spacing);
        assert_eq!(t.font_size, Theme::dark().font_size);
        assert_eq!(t.icon_size, Theme::dark().icon_size);
        assert_eq!(t.tile_size, Theme::dark().tile_size);
        assert_eq!(t.tile_radius, Theme::dark().tile_radius);
        assert_eq!(t.show_descriptions, Theme::dark().show_descriptions);
    }
}
