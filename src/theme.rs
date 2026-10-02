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
/// Use builders: `Theme::dark().rounded(12.0).width(560.0)`.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Window width in logical pixels.
    pub width: f32,
    /// Window height in logical pixels.
    pub height: f32,
    /// Max rows visible before scrolling.
    pub max_visible_items: usize,
    /// Row height in logical pixels.
    pub row_height: f32,
    /// Icon size in logical pixels.
    pub icon_size: f32,
    /// Tile edge length in logical pixels (used with [`Layout::Grid`]).
    pub tile_size: f32,
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
    /// Dark theme (default).
    pub fn dark() -> Self {
        Self {
            width: 560.0,
            height: 380.0,
            max_visible_items: 8,
            row_height: 44.0,
            icon_size: 28.0,
            tile_size: 112.0,
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

    /// Light theme.
    pub fn light() -> Self {
        Self {
            background: Color32::from_rgb(245, 245, 248),
            surface: Color32::WHITE,
            selected: Color32::from_rgb(50, 110, 230),
            text: Color32::from_rgb(20, 20, 25),
            dim_text: Color32::from_rgb(110, 110, 125),
            accent: Color32::from_rgb(40, 90, 220),
            border: Color32::from_rgb(210, 210, 220),
            ..Self::dark()
        }
    }

    /// Transparent dark theme.
    pub fn transparent_dark() -> Self {
        Self {
            background: Color32::from_rgba_premultiplied(20, 20, 26, 230),
            surface: Color32::from_rgba_premultiplied(35, 35, 45, 230),
            ..Self::dark()
        }
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }
    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }
    pub fn rounded(mut self, r: f32) -> Self {
        self.corner_radius = r;
        self
    }
    pub fn padding(mut self, p: f32) -> Self {
        self.padding = p;
        self
    }
    pub fn margin(mut self, m: f32) -> Self {
        self.margin = m;
        self
    }
    pub fn max_visible_items(mut self, n: usize) -> Self {
        self.max_visible_items = n;
        self
    }
    pub fn font_size(mut self, s: f32) -> Self {
        self.font_size = s;
        self
    }
    pub fn accent(mut self, c: Color32) -> Self {
        self.accent = c;
        self.selected = c;
        self
    }
    pub fn background(mut self, c: Color32) -> Self {
        self.background = c;
        self
    }
    pub fn border(mut self, color: Color32, width: f32) -> Self {
        self.border = color;
        self.border_width = width;
        self
    }
    pub fn show_descriptions(mut self, yes: bool) -> Self {
        self.show_descriptions = yes;
        self
    }
    pub fn tile_size(mut self, s: f32) -> Self {
        self.tile_size = s;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn margin_defaults_and_settable() {
        assert_eq!(Theme::dark().margin, 8.0);
        assert_eq!(Theme::dark().margin(0.0).margin, 0.0);
        assert_eq!(Theme::dark().margin(24.0).margin, 24.0);
    }
}
