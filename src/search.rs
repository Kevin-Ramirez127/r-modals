use crate::ModalOption;

/// Search / filter configuration.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Show the search field. If false, all options are listed unfiltered.
    pub enabled: bool,
    /// Placeholder (hint) text inside the search field.
    pub placeholder: String,
    /// `true` = `Foo` does NOT match `foo`. Defaults to false.
    pub case_sensitive: bool,
    /// Auto-focus the search field on open. Defaults to true.
    pub autofocus: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            placeholder: "Type to filter…".to_string(),
            case_sensitive: false,
            autofocus: true,
        }
    }
}

/// Pure, GUI-free filtering used by the app and unit tests.
///
/// Substring match over label + id + description.
/// Returns indices into `options`, preserving order.
/// A parent option also matches when any of its descendants match (so it
/// can act as a gateway to drill into).
///
/// The query is lowered once here; each option is checked field-by-field
/// with early exit (label hit skips id/description), so matching options
/// cost a single short `to_lowercase` instead of a full haystack build.
pub fn filter_options(options: &[ModalOption], query: &str, case_sensitive: bool) -> Vec<usize> {
    if query.is_empty() {
        return (0..options.len()).collect();
    }
    if case_sensitive {
        options
            .iter()
            .enumerate()
            .filter(|(_, o)| o.matches_query(query, "", true))
            .map(|(i, _)| i)
            .collect()
    } else {
        let q = query.to_lowercase();
        options
            .iter()
            .enumerate()
            .filter(|(_, o)| o.matches_query(query, &q, false))
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Vec<ModalOption> {
        vec![
            ModalOption::new("ff", "Firefox").description("Web browser"),
            ModalOption::new("term", "Terminal").description("Shell"),
            ModalOption::new("files", "Files"),
        ]
    }

    #[test]
    fn empty_query_returns_all() {
        assert_eq!(filter_options(&opts(), "", false), vec![0, 1, 2]);
    }

    #[test]
    fn case_insensitive_by_default() {
        assert_eq!(filter_options(&opts(), "firefox", false), vec![0]);
        assert_eq!(filter_options(&opts(), "FIREFOX", false), vec![0]);
    }

    #[test]
    fn case_sensitive_respected() {
        assert_eq!(filter_options(&opts(), "Firefox", true), vec![0]);
        assert!(filter_options(&opts(), "firefox", true).is_empty());
        // description is also case-sensitive
        assert!(filter_options(&opts(), "web browser", true).is_empty());
        assert_eq!(filter_options(&opts(), "Web browser", true), vec![0]);
    }

    #[test]
    fn matches_id_and_description() {
        assert_eq!(filter_options(&opts(), "term", false), vec![1]);
        assert_eq!(filter_options(&opts(), "shell", false), vec![1]);
    }

    #[test]
    fn unsearchable_excluded() {
        let mut o = opts();
        o[0] = ModalOption::new("ff", "Firefox").unsearchable(true);
        assert!(filter_options(&o, "firefox", false).is_empty());
        // …but still listed with an empty query
        assert_eq!(filter_options(&o, "", false), vec![0, 1, 2]);
    }

    #[test]
    fn parent_matches_descendant_query() {
        let o = vec![
            ModalOption::new("disp", "Display")
                .sub_option(ModalOption::new("res", "Resolution")),
            ModalOption::new("snd", "Sound"),
        ];
        assert_eq!(filter_options(&o, "resolution", false), vec![0]);
        assert_eq!(filter_options(&o, "Resolution", true), vec![0]);
        assert_eq!(filter_options(&o, "sound", false), vec![1]);
        assert!(filter_options(&o, "wifi", false).is_empty());
    }
}
