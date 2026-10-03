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
    ///
    /// NOTE: this can hash full image bytes — call it once per option and
    /// reuse the result (see `ModalApp::icon_keys`). Calling it per frame
    /// would re-hash megabytes of pixel data at 60fps.
    pub(crate) fn cache_key(&self, option_id: &str) -> Option<String> {
        match self {
            IconSource::None => None,
            IconSource::File(p) => Some(format!("icon:{option_id}:file:{}", p.display())),
            IconSource::Bytes(b) => Some(format!("icon:{option_id}:bytes:{}:{:x}", b.len(), {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                b.hash(&mut h);
                h.finish()
            })),
            IconSource::Pixels {
                width,
                height,
                rgba,
            } => {
                // Include a content hash: same dims + same id but different
                // pixels must not share a stale texture.
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                rgba.hash(&mut h);
                Some(format!(
                    "icon:{option_id}:px:{width}x{height}:{:x}",
                    h.finish()
                ))
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

impl From<&[u8]> for IconSource {
    fn from(b: &[u8]) -> Self {
        IconSource::Bytes(b.to_vec())
    }
}

/// One selectable row in the modal.
///
/// An option with [`ModalOption::sub_option`] children acts as a submenu:
/// activating it drills one level deeper instead of picking it. Only
/// childless (leaf) options can be picked.
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
    /// (A hidden parent still surfaces as a gateway when one of its
    /// descendants matches the query.)
    pub unsearchable: bool,
    /// Nested options shown one level deeper on drill-down.
    /// Empty (default) means this option is a pickable leaf.
    pub children: Vec<ModalOption>,
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
            children: Vec::new(),
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

    /// Nest one sub-option under this option, turning it into a submenu.
    /// Activating a parent drills one level deeper; only leaves can be
    /// picked. Nests arbitrarily deep.
    pub fn sub_option(mut self, opt: ModalOption) -> Self {
        self.children.push(opt);
        self
    }

    /// Nest many sub-options under this option (see [`ModalOption::sub_option`]).
    pub fn sub_options(mut self, opts: impl IntoIterator<Item = ModalOption>) -> Self {
        self.children.extend(opts);
        self
    }

    /// True when this option has children (a submenu: activates to drill
    /// in rather than picking).
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Text haystack used for filtering (label + id + description).
    /// Prefer [`ModalOption::matches_query`] in hot paths: it checks each
    /// field separately and short-circuits, avoiding this allocation.
    pub(crate) fn haystack(&self) -> String {
        match &self.description {
            Some(d) => format!("{} {} {}", self.label, self.id, d),
            None => format!("{} {}", self.label, self.id),
        }
    }

    /// Lowercased haystack, precomputed once per modal for the
    /// case-insensitive fast path (see `ModalApp` filter cache).
    /// Includes descendant text so a parent matches when any of its
    /// children match (it then acts as a gateway to drill into).
    pub(crate) fn haystack_lower(&self) -> String {
        let mut s = self.haystack().to_lowercase();
        for c in &self.children {
            s.push(' ');
            s.push_str(&c.haystack_lower());
        }
        s
    }

    /// Substring match over label + id + description without building the
    /// combined haystack. Case-insensitive callers pass the already-lowered
    /// query; each field is lowered lazily and checked in order so a label
    /// hit skips the id/description work entirely.
    /// A parent also matches when any descendant matches (gateway to drill
    /// into), even if the parent itself is `unsearchable`.
    pub(crate) fn matches_query(&self, query: &str, query_lower: &str, case_sensitive: bool) -> bool {
        let own = if self.unsearchable {
            false
        } else if case_sensitive {
            self.label.contains(query)
                || self.id.contains(query)
                || self
                    .description
                    .as_deref()
                    .is_some_and(|d| d.contains(query))
        } else {
            // `to_lowercase` allocates; do it field-by-field with early exit
            // so a label match never touches id/description.
            self.label.to_lowercase().contains(query_lower)
                || self.id.to_lowercase().contains(query_lower)
                || self
                    .description
                    .as_deref()
                    .is_some_and(|d| d.to_lowercase().contains(query_lower))
        };
        own || self
            .children
            .iter()
            .any(|c| c.matches_query(query, query_lower, case_sensitive))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_cache_key_distinguishes_content() {
        let a = IconSource::Bytes(vec![0u8; 16]);
        let b = IconSource::Bytes(vec![1u8; 16]);
        assert_ne!(a.cache_key("x"), b.cache_key("x"));
        assert_eq!(
            a.cache_key("x"),
            IconSource::Bytes(vec![0u8; 16]).cache_key("x")
        );
    }

    #[test]
    fn byte_slice_into_icon() {
        let bytes: &[u8] = &[1, 2, 3];
        let icon: IconSource = bytes.into();
        assert!(matches!(icon, IconSource::Bytes(_)));
    }

    #[test]
    fn sub_options_nest_and_report_children() {
        let leaf = ModalOption::new("l", "Leaf");
        assert!(!leaf.has_children());
        let parent = ModalOption::new("p", "Parent")
            .sub_option(leaf)
            .sub_options([
                ModalOption::new("a", "A"),
                ModalOption::new("b", "B"),
            ]);
        assert!(parent.has_children());
        assert_eq!(parent.children.len(), 3);
        assert_eq!(parent.children[0].id, "l");
    }

    #[test]
    fn parent_matches_via_descendant() {
        let parent = ModalOption::new("disp", "Display").sub_option(
            ModalOption::new("res", "Resolution").sub_option(ModalOption::new("4k", "4K Ultra")),
        );
        // Own text matches.
        assert!(parent.matches_query("Display", "display", false));
        assert!(parent.matches_query("Display", "", true));
        // Descendant text matches (gateway), both modes.
        assert!(parent.matches_query("4k ultra", "4k ultra", false));
        assert!(parent.matches_query("4K Ultra", "", true));
        assert!(parent.haystack_lower().contains("4k ultra"));
        // Unrelated query matches nothing.
        assert!(!parent.matches_query("wifi", "wifi", false));
        assert!(!parent.matches_query("wifi", "", true));
    }

    #[test]
    fn unsearchable_parent_is_still_a_gateway() {
        let parent = ModalOption::new("p", "Parent")
            .unsearchable(true)
            .sub_option(ModalOption::new("c", "Child"));
        assert!(!parent.matches_query("Parent", "parent", false));
        assert!(parent.matches_query("Child", "child", false));
    }
}
