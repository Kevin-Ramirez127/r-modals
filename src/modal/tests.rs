//! Unit tests for [`super`].

use super::*;

#[test]
fn center_on_navigate_defaults_on_and_is_settable() {
    assert!(Modal::new("x").center_on_navigate);
    assert!(!Modal::new("x").center_on_navigate(false).center_on_navigate);
}

#[test]
fn fit_width_defaults_off_and_is_settable() {
    assert!(!Modal::new("x").fit_width_to_content);
    assert!(
        Modal::new("x")
            .fit_width_to_content(true)
            .fit_width_to_content
    );
}

#[test]
fn collect_actions_walks_nested_paths() {
    let m = Modal::new("x")
        .option(ModalOption::new("a", "A").command("echo-a"))
        .option(ModalOption::new("p", "Parent").sub_options([
            ModalOption::new("b", "B"),
            ModalOption::new("c", "C").shell("echo-c"),
        ]));
    let actions = m.collect_actions();
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0].0, vec![0]);
    assert!(matches!(actions[0].1, crate::SelectAction::Command { .. }));
    assert_eq!(actions[1].0, vec![1, 1]);
    assert!(matches!(actions[1].1, crate::SelectAction::Shell(_)));
}

#[test]
fn dispatch_action_falls_back_on_spawn_failure() {
    use std::sync::{Arc, Mutex};
    let sel = ModalSelection {
        id: "bad".to_string(),
        label: "Bad".to_string(),
        index: 0,
        path: vec![0],
        query: String::new(),
        custom_input: false,
    };
    // No panic without a handler either.
    let bare = Modal::new("y")
        .option(ModalOption::new("bad", "Bad").command("definitely-not-a-real-binary"));
    let bare_actions = bare.collect_actions();
    Modal::dispatch_action(&bare_actions, &sel);

    // With a handler: it fires exactly once, with the pick and the error.
    let fired = Arc::new(Mutex::new(Vec::new()));
    let fired2 = fired.clone();
    let m2 = Modal::new("x").option(
        ModalOption::new("bad", "Bad")
            .command("definitely-not-a-real-binary")
            .on_error(move |sel, err| {
                fired2
                    .lock()
                    .unwrap()
                    .push((sel.id.clone(), err.to_string()));
            }),
    );
    Modal::dispatch_action(&m2.collect_actions(), &sel);
    let fired = fired.lock().unwrap();
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].0, "bad");
    assert!(!fired[0].1.is_empty());
}

#[test]
fn dispatch_action_skips_fallback_on_success() {
    use std::sync::{Arc, Mutex};
    let fired = Arc::new(Mutex::new(false));
    let fired2 = fired.clone();
    // Callbacks cannot spawn-fail: fallback must stay quiet.
    let m = Modal::new("x").option(
        ModalOption::new("ok", "Ok")
            .on_select(|_| {})
            .on_error(move |_, _| *fired2.lock().unwrap() = true),
    );
    let sel = ModalSelection {
        id: "ok".to_string(),
        label: "Ok".to_string(),
        index: 0,
        path: vec![0],
        query: String::new(),
        custom_input: false,
    };
    Modal::dispatch_action(&m.collect_actions(), &sel);
    assert!(!*fired.lock().unwrap());
}

#[test]
fn stylesheet_override_wins_over_stylesheet_and_theme() {
    let m = Modal::new("x")
        .theme(Theme::from_css("modal { width: 100px; color: red; }").unwrap())
        .stylesheet("modal { width: 200px; color: blue; }")
        .stylesheet_override("modal { width: 300px; }");
    let t = m.resolved_theme().unwrap();
    // Override wins where specified…
    assert_eq!(t.width, 300.0);
    // …stylesheet wins where the override is silent…
    assert_eq!(t.text, egui::Color32::from_rgb(0, 0, 255));
    // …and a bad override errors like a bad stylesheet.
    assert!(Modal::new("x")
        .stylesheet_override("modal { width: 10em; }")
        .resolved_theme()
        .is_err());
    // No override: previous behavior unchanged.
    let t = Modal::new("x")
        .stylesheet("modal { width: 200px; }")
        .resolved_theme()
        .unwrap();
    assert_eq!(t.width, 200.0);
}

#[test]
fn stylesheet_layers_over_theme_and_bad_css_errors() {
    let base = Theme::from_css("modal { width: 100px; }").unwrap();
    let m = Modal::new("x")
        .theme(base)
        .stylesheet("modal { width: 200px; background: #000000; }");
    let t = m.resolved_theme().unwrap();
    assert_eq!(t.width, 200.0);
    assert_eq!(t.background, egui::Color32::from_rgb(0, 0, 0));
    // Untouched fields keep the custom theme, not the dark default.
    assert_eq!(t.height, Theme::dark().height);
    assert!(Modal::new("x")
        .stylesheet("modal { width: 10px; color }")
        .resolved_theme()
        .is_err());
}

#[test]
fn new_builder_defaults() {
    let m = Modal::new("x");
    assert!(m.default_selected.is_none());
    assert!(!m.allow_custom_input);
    assert!(!m.multi_select);
    assert!(m.max_results.is_none());
    assert!(m.footer_hint.is_none());
    assert!(m.fonts.is_empty());
    assert!(m.history_file.is_none());
    assert!(m.shortcut_sources.is_empty());
    assert!(m.compiled_shortcuts.is_empty());
    assert!(m.auto_shortcuts);
    assert!(!m.search.fuzzy);
    assert_eq!(m.empty_message, "No matches");
}

#[test]
fn resolve_shortcuts_fills_hints_and_drops_unknown_ids() {
    let mut m = Modal::new("x")
        .option(ModalOption::new("ff", "Firefox"))
        .option(ModalOption::new("term", "Terminal").shortcut("Ctrl+T"))
        .shortcuts_toml(
            r#"
            [[shortcut]]
            key = "Ctrl+O"
            option = "ff"

            [[shortcut]]
            key = "Ctrl+Alt+T"
            option = "term"

            [[shortcut]]
            key = "Ctrl+Q"
            option = "ghost"
            "#,
        );
    m.resolve_shortcuts().unwrap();
    // A matching row gains the binding hint; an explicit one is kept.
    assert_eq!(m.options[0].shortcut.as_deref(), Some("Ctrl+O"));
    assert_eq!(m.options[1].shortcut.as_deref(), Some("Ctrl+T"));
    // The ghost-id shortcut is dropped: only the two real ones remain.
    assert_eq!(m.compiled_shortcuts.len(), 2);
    assert_eq!(m.compiled_shortcuts[0].hint, "Ctrl+Alt+T");
}

#[test]
fn resolve_shortcuts_command_target_and_errors() {
    let mut m = Modal::new("x")
        .option(ModalOption::new("a", "A"))
        .shortcuts_toml(
            r#"
            [[shortcut]]
            key = "F5"
            command = "echo"
            args = ["hi"]
            "#,
        );
    m.resolve_shortcuts().unwrap();
    assert!(matches!(
        m.compiled_shortcuts[0].target,
        crate::shortcut::ShortcutTarget::Action(crate::SelectAction::Command { .. })
    ));

    // Invalid TOML surfaces as ModalError::Shortcuts.
    let mut bad = Modal::new("x")
        .option(ModalOption::new("a", "A"))
        .shortcuts_toml("[[shortcut]");
    assert!(matches!(
        bad.resolve_shortcuts(),
        Err(ModalError::Shortcuts(_))
    ));

    // Unknown key names surface too.
    let mut bad_key = Modal::new("x")
        .option(ModalOption::new("a", "A"))
        .shortcuts_toml("[[shortcut]]\nkey = \"Ctrl+Nope\"\noption = \"a\"\n");
    assert!(matches!(
        bad_key.resolve_shortcuts(),
        Err(ModalError::Shortcuts(_))
    ));
}

#[test]
fn programmatic_shortcut_and_auto_toggle() {
    let mut m = Modal::new("x")
        .option(ModalOption::new("a", "A"))
        .shortcut(crate::ShortcutSpec::option("Ctrl+A", "a"));
    assert_eq!(m.compiled_shortcuts.len(), 0);
    m.resolve_shortcuts().unwrap();
    assert_eq!(m.compiled_shortcuts.len(), 1);
    assert_eq!(m.options[0].shortcut.as_deref(), Some("Ctrl+A"));
    assert!(Modal::new("x").auto_shortcuts);
    assert!(!Modal::new("x").auto_shortcuts(false).auto_shortcuts);
}

#[test]
fn error_converts_to_string_for_legacy_mains() {
    let e = crate::ModalError::NoOptions;
    let s: String = e.into();
    assert!(s.contains("no options"));
    assert_eq!(
        Modal::new("x").run().unwrap_err().to_string(),
        "r-modals: no options provided"
    );
}

#[test]
fn max_results_truncates_filtered_indices() {
    let m = Modal::new("x")
        .options((0..10).map(|i| ModalOption::new(format!("id{i}"), format!("Item {i}"))))
        .max_results(3);
    assert_eq!(m.filtered_indices("Item").len(), 3);
}

#[test]
fn history_sorts_most_used_first() {
    let dir = std::env::temp_dir().join(format!("qm-hist-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("hist.txt");
    std::fs::write(&path, "b 5\na 1\n").unwrap();
    let mut m = Modal::new("x")
        .option(ModalOption::new("a", "A"))
        .option(ModalOption::new("b", "B"))
        .option(ModalOption::new("c", "C"))
        .history_file(path.to_str().unwrap());
    m.apply_history();
    assert_eq!(m.options[0].id, "b");
    assert_eq!(m.options[1].id, "a");
    assert_eq!(m.options[2].id, "c");
    m.record_history("c");
    let counts = Modal::load_history(path.to_str().unwrap());
    assert_eq!(counts.get("c"), Some(&1));
    assert_eq!(counts.get("b"), Some(&5));
    let _ = std::fs::remove_dir_all(&dir);
}
