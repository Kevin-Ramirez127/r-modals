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
    /// `true` = subsequence fuzzy match (ranked best-first) instead of
    /// substring match. Defaults to false (previous behavior).
    pub fuzzy: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            placeholder: "Type to filter…".to_string(),
            case_sensitive: false,
            autofocus: true,
            fuzzy: false,
        }
    }
}

/// Fuzzy subsequence score: every char of `query` must appear in `text`
/// in order. Higher is better: consecutive runs, word-start hits, and
/// early/short candidates score more. Returns `None` on no match.
pub fn fuzzy_score(text: &str, query: &str, case_sensitive: bool) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let (t, q): (Vec<char>, Vec<char>) = if case_sensitive {
        (text.chars().collect(), query.chars().collect())
    } else {
        (
            text.to_lowercase().chars().collect(),
            query.to_lowercase().chars().collect(),
        )
    };
    let mut ti = 0usize;
    let mut qi = 0usize;
    let mut score: i32 = 0;
    let mut run: i32 = 0;
    let mut first_at: Option<usize> = None;
    while ti < t.len() && qi < q.len() {
        if t[ti] == q[qi] {
            if first_at.is_none() {
                first_at = Some(ti);
            }
            run += 1;
            // Consecutive matches compound; word-start matches get a bonus.
            score += 10 + run * 5;
            if ti == 0
                || t[ti - 1] == ' '
                || t[ti - 1] == '-'
                || t[ti - 1] == '_'
                || t[ti - 1] == '/'
            {
                score += 8;
            }
            qi += 1;
        } else {
            run = 0;
        }
        ti += 1;
    }
    if qi != q.len() {
        return None;
    }
    let first = first_at.unwrap_or(0) as i32;
    let len_penalty = t.len() as i32;
    Some(score - first * 2 - len_penalty / 8)
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
    filter_options_with(
        options,
        query,
        &SearchConfig {
            case_sensitive,
            ..SearchConfig::default()
        },
    )
}

/// Config-aware filtering: substring (default, order-preserving) or fuzzy
/// (`fuzzy: true`, ranked best-first, stable on ties).
pub fn filter_options_with(options: &[ModalOption], query: &str, cfg: &SearchConfig) -> Vec<usize> {
    if query.is_empty() {
        return (0..options.len()).collect();
    }
    if cfg.fuzzy {
        let mut scored: Vec<(usize, i32)> = options
            .iter()
            .enumerate()
            .filter_map(|(i, o)| o.fuzzy_score(query, cfg.case_sensitive).map(|s| (i, s)))
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.into_iter().map(|(i, _)| i).collect()
    } else if cfg.case_sensitive {
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
            ModalOption::new("disp", "Display").sub_option(ModalOption::new("res", "Resolution")),
            ModalOption::new("snd", "Sound"),
        ];
        assert_eq!(filter_options(&o, "resolution", false), vec![0]);
        assert_eq!(filter_options(&o, "Resolution", true), vec![0]);
        assert_eq!(filter_options(&o, "sound", false), vec![1]);
        assert!(filter_options(&o, "wifi", false).is_empty());
    }

    #[test]
    fn fuzzy_ranks_best_first() {
        use super::SearchConfig;
        let o = vec![
            ModalOption::new("x", "Xylophone fuzzy finder"),
            ModalOption::new("ff", "Firefox"),
        ];
        let cfg = SearchConfig {
            fuzzy: true,
            ..SearchConfig::default()
        };
        // "ff" is a subsequence of both, but Firefox matches earlier/tighter.
        assert_eq!(filter_options_with(&o, "ff", &cfg), vec![1, 0]);
        // Non-subsequence matches nothing.
        assert!(filter_options_with(&o, "zzz", &cfg).is_empty());
        // Empty query keeps order.
        assert_eq!(filter_options_with(&o, "", &cfg), vec![0, 1]);
    }

    #[test]
    fn fuzzy_scores_basics() {
        assert!(super::fuzzy_score("Firefox", "ffx", false).is_some());
        assert!(super::fuzzy_score("Firefox", "fxz", false).is_none());
        assert!(super::fuzzy_score("Firefox", "", false) == Some(0));
        // Case-insensitive by default, sensitive on demand.
        assert!(super::fuzzy_score("Firefox", "FIREFOX", false).is_some());
        assert!(super::fuzzy_score("Firefox", "firefox", true).is_none());
    }

    #[test]
    fn keywords_match_both_modes() {
        use super::SearchConfig;
        let o = vec![ModalOption::new("ff", "Browser").keyword("firefox")];
        assert_eq!(filter_options(&o, "firefox", false), vec![0]);
        let cfg = SearchConfig {
            fuzzy: true,
            ..SearchConfig::default()
        };
        assert_eq!(filter_options_with(&o, "ffx", &cfg), vec![0]);
    }
}
