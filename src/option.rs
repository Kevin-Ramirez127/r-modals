use std::path::{Path, PathBuf};

/// Image attached to an option.
///
/// Loaded lazily at render time via the `image` crate; failures
/// fall back to no icon instead of crashing the modal.
#[derive(Debug, Clone, Default)]
pub enum IconSource {
    /// No image.
    #[default]
    None,
    /// Load from a file path (png/jpg/svg-bitmap/etc. supported by `image`).
    File(PathBuf),
    /// Raw encoded bytes (e.g. `include_bytes!("icon.png")`).
    Bytes(Vec<u8>),
    /// Raw RGBA pixels.
    Pixels {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
    },
}

impl IconSource {
    /// Stable cache key for the egui texture cache.
    pub(crate) fn cache_key(&self, option_id: &str) -> Option<String> {
        match self {
            IconSource::None => None,
            IconSource::File(p) => Some(format!("icon:{option_id}:file:{}", p.display())),
            IconSource::Bytes(b) => Some(format!("icon:{option_id}:bytes:{}", b.len())),
            IconSource::Pixels { width, height, .. } => {
                Some(format!("icon:{option_id}:px:{width}x{height}"))
            }
        }
    }
}

impl From<&str> for IconSource {
    fn from(s: &str) -> Self {
        IconSource::File(PathBuf::from(s))
    }
}

impl From<&Path> for IconSource {
    fn from(p: &Path) -> Self {
        IconSource::File(p.to_path_buf())
    }
}

impl From<PathBuf> for IconSource {
    fn from(p: PathBuf) -> Self {
        IconSource::File(p)
    }
}

impl From<Vec<u8>> for IconSource {
    fn from(b: Vec<u8>) -> Self {
        IconSource::Bytes(b)
    }
}

/// One selectable row in the modal.
#[derive(Debug, Clone)]
pub struct ModalOption {
    /// Stable id returned on selection.
    pub id: String,
    /// Primary text (what search matches against).
    pub label: String,
    /// Secondary dimmed text.
    pub description: Option<String>,
    /// Optional image shown left of the label.
    pub icon: IconSource,
    /// Hides the row from search but keeps it selectable via arrow keys.
    /// Useful for separators/headers. Defaults to false.
    pub unsearchable: bool,
}

impl ModalOption {
    /// Create an option. `id` is returned in [`crate::ModalSelection`].
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            icon: IconSource::None,
            unsearchable: false,
        }
    }

    /// Secondary text shown under / beside the label.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Attach an image. Accepts `IconSource`, `&str`/`PathBuf` (file), or `Vec<u8>` (bytes).
    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.icon = icon.into();
        self
    }

    /// Raw RGBA icon.
    pub fn icon_pixels(mut self, width: usize, height: usize, rgba: Vec<u8>) -> Self {
        self.icon = IconSource::Pixels {
            width,
            height,
            rgba,
        };
        self
    }

    /// Exclude this row from text filtering.
    pub fn unsearchable(mut self, yes: bool) -> Self {
        self.unsearchable = yes;
        self
    }

    /// Text haystack used for filtering (label + id + description).
    pub(crate) fn haystack(&self) -> String {
        match &self.description {
            Some(d) => format!("{} {} {}", self.label, self.id, d),
            None => format!("{} {}", self.label, self.id),
        }
    }
}
