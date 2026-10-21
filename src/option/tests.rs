//! Unit tests for [`super`].

use super::*;

#[test]
fn find_option_path_walks_submenus() {
    let opts = vec![
        ModalOption::new("a", "A"),
        ModalOption::new("p", "Parent").sub_options([
            ModalOption::new("b", "B"),
            ModalOption::new("c", "C").sub_option(ModalOption::new("deep", "Deep")),
        ]),
    ];
    assert_eq!(find_option_path(&opts, "a"), Some(vec![0]));
    assert_eq!(find_option_path(&opts, "p"), Some(vec![1]));
    assert_eq!(find_option_path(&opts, "c"), Some(vec![1, 1]));
    assert_eq!(find_option_path(&opts, "deep"), Some(vec![1, 1, 0]));
    assert_eq!(find_option_path(&opts, "missing"), None);
}

#[test]
fn option_at_mut_reaches_nested_option() {
    let mut opts = vec![ModalOption::new("p", "Parent").sub_option(ModalOption::new("c", "Child"))];
    option_at_mut(&mut opts, &[0, 0]).unwrap().shortcut = Some("Ctrl+C".to_string());
    assert_eq!(opts[0].children[0].shortcut.as_deref(), Some("Ctrl+C"));
    assert!(option_at_mut(&mut opts, &[9]).is_none());
    assert!(option_at_mut(&mut opts, &[]).is_none());
}

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
        .sub_options([ModalOption::new("a", "A"), ModalOption::new("b", "B")]);
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

#[test]
fn disabled_and_separator_never_activate() {
    assert!(!ModalOption::new("x", "X").disabled(true).activatable());
    assert!(!ModalOption::separator("s", "Section").activatable());
    assert!(ModalOption::new("x", "X").activatable());
}

#[test]
fn separator_never_matches_filtered_query() {
    let s = ModalOption::separator("s", "Section");
    assert!(!s.matches_query("Section", "section", false));
    assert!(s.fuzzy_score("sec", false).is_none());
}

#[test]
fn shortcut_and_keywords_builders() {
    let o = ModalOption::new("x", "X")
        .shortcut("Ctrl+O")
        .keyword("extra")
        .keywords(["more", "stuff"]);
    assert_eq!(o.shortcut.as_deref(), Some("Ctrl+O"));
    assert_eq!(o.keywords, vec!["extra", "more", "stuff"]);
    assert!(o.matches_query("stuff", "stuff", false));
}

#[test]
fn action_builders() {
    assert!(ModalOption::new("x", "X").action.is_none());
    let c = ModalOption::new("ff", "Firefox")
        .command("firefox")
        .arg("--new-window")
        .args(["--safe-mode"]);
    match &c.action {
        Some(SelectAction::Command { program, args }) => {
            assert_eq!(program, "firefox");
            assert_eq!(args, &["--new-window", "--safe-mode"]);
        }
        other => panic!("expected command, got {other:?}"),
    }
    // args without a command: no-op, stays None.
    assert!(ModalOption::new("x", "X").arg("--foo").action.is_none());
    let s = ModalOption::new("up", "Update").shell("apt update");
    assert!(matches!(s.action, Some(SelectAction::Shell(_))));
    let cb = ModalOption::new("hi", "Hi").on_select(|_| {});
    assert!(matches!(cb.action, Some(SelectAction::Callback(_))));
    // Debug works with every variant (callbacks aren't introspectable).
    assert!(format!("{cb:?}").contains("Callback"));
}

#[test]
fn callback_executes_with_selection() {
    use std::sync::{Arc, Mutex};
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    let opt = ModalOption::new("ff", "Firefox")
        .command("firefox")
        .on_select(move |sel| seen2.lock().unwrap().push(sel.id.clone()));
    // `on_select` replaces the command.
    assert!(matches!(opt.action, Some(SelectAction::Callback(_))));
    let sel = crate::ModalSelection {
        id: "ff".to_string(),
        label: "Firefox".to_string(),
        index: 0,
        path: vec![0],
        query: String::new(),
        custom_input: false,
        shortcut: None,
    };
    opt.action.as_ref().unwrap().execute(&sel).unwrap();
    assert_eq!(*seen.lock().unwrap(), vec!["ff".to_string()]);
}

#[test]
fn on_error_defaults_none_and_debugs() {
    assert!(ModalOption::new("x", "X").on_error.is_none());
    let opt = ModalOption::new("term", "Terminal")
        .command("ghost-terminal")
        .on_error(|_, _| {});
    assert!(opt.on_error.is_some());
    assert!(format!("{opt:?}").contains("on_error"));
    // Failing spawn surfaces as Err from execute.
    let sel = crate::ModalSelection {
        id: "term".to_string(),
        label: "Terminal".to_string(),
        index: 0,
        path: vec![0],
        query: String::new(),
        custom_input: false,
        shortcut: None,
    };
    assert!(opt.action.as_ref().unwrap().execute(&sel).is_err());
}
