//! Unit tests for [`super`].

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
