use egui::Color32;
use std::sync::OnceLock;

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
    /// Search field font size.
    pub font_size: f32,
    /// Search field background fill.
    pub search_background: Color32,
    /// Search field typed text color.
    pub search_text: Color32,
    /// Search field placeholder (hint) text color.
    pub search_hint: Color32,
    /// Search field corner radius. `None` (default) derives 60% of
    /// `corner_radius`, so it follows the modal shape automatically.
    pub search_radius: Option<f32>,
    /// Search field font size. `None` (default) follows `font_size`.
    pub search_font_size: Option<f32>,
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
            font_size: 15.0,
            search_background: Color32::from_rgb(34, 34, 40),
            search_text: Color32::from_rgb(235, 235, 240),
            search_hint: Color32::from_rgb(150, 150, 165),
            search_radius: None,
            search_font_size: None,
            show_descriptions: true,
        }
    }

    /// Dark theme (default), defined by the embedded `dark.css` stylesheet.
    /// The parsed preset is cached process-wide: every `Modal::new` used to
    /// re-parse the CSS via `lightningcss`.
    pub fn dark() -> Self {
        dark_preset().clone()
    }

    /// Light theme, defined by the embedded `light.css` stylesheet.
    /// Parsed once and cached (see [`Theme::dark`]).
    pub fn light() -> Self {
        light_preset().clone()
    }

    /// Transparent dark theme, defined by the embedded
    /// `transparent_dark.css` stylesheet.
    /// Parsed once and cached (see [`Theme::dark`]).
    pub fn transparent_dark() -> Self {
        transparent_dark_preset().clone()
    }

    /// Best-effort system theme: returns [`Theme::light`] when the
    /// environment looks like a light setup (`COLORFGBG` ending in
    /// `;15`, `;7`, or `THEME=light`), otherwise [`Theme::dark`].
    /// No desktop-portal query is done — deterministic and headless-safe.
    pub fn auto() -> Self {
        let light = std::env::var("QUICK_MODALS_THEME")
            .map(|v| v.eq_ignore_ascii_case("light"))
            .unwrap_or(false)
            || std::env::var("COLORFGBG").is_ok_and(|v| {
                v.rsplit(';')
                    .next()
                    .is_some_and(|bg| matches!(bg.trim(), "15" | "7" | "11" | "12" | "14"))
            });
        if light {
            Self::light()
        } else {
            Self::dark()
        }
    }

    /// Effective option corner radius: explicit override or 60% of the modal radius.
    pub(crate) fn effective_tile_radius(&self) -> f32 {
        self.tile_radius.unwrap_or(self.corner_radius * 0.6)
    }

    /// Effective search field corner radius: explicit override or 60% of
    /// the modal radius.
    pub(crate) fn effective_search_radius(&self) -> f32 {
        self.search_radius.unwrap_or(self.corner_radius * 0.6)
    }

    /// Effective search field font size: explicit override or the modal
    /// font size.
    pub(crate) fn effective_search_font_size(&self) -> f32 {
        self.search_font_size.unwrap_or(self.font_size)
    }

    /// Build a theme from a CSS stylesheet (see `css` module docs for the
    /// supported selectors and properties). Starts from the dark defaults.
    ///
    /// ```no_run
    /// use r_modals::Theme;
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

fn dark_preset() -> &'static Theme {
    static DARK: OnceLock<Theme> = OnceLock::new();
    DARK.get_or_init(|| {
        let mut theme = Theme::base();
        theme
            .apply_css(include_str!("../assets/themes/dark.css"))
            .expect("built-in dark.css is valid");
        theme
    })
}

fn light_preset() -> &'static Theme {
    static LIGHT: OnceLock<Theme> = OnceLock::new();
    LIGHT.get_or_init(|| {
        let mut theme = Theme::base();
        theme
            .apply_css(include_str!("../assets/themes/light.css"))
            .expect("built-in light.css is valid");
        theme
    })
}

fn transparent_dark_preset() -> &'static Theme {
    static TD: OnceLock<Theme> = OnceLock::new();
    TD.get_or_init(|| {
        let mut theme = Theme::base();
        theme
            .apply_css(include_str!("../assets/themes/transparent_dark.css"))
            .expect("built-in transparent_dark.css is valid");
        theme
    })
}

#[cfg(test)]
mod tests;
