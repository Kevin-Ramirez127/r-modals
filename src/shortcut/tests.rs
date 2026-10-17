//! Unit tests for [`super`].

use super::*;

fn mods(ctrl: bool, shift: bool, alt: bool) -> egui::Modifiers {
    egui::Modifiers {
        ctrl,
        shift,
        alt,
        ..egui::Modifiers::NONE
    }
}

#[test]
fn parses_simple_and_modified_combos() {
    assert_eq!(
        parse_combo("O").unwrap(),
        (egui::Modifiers::NONE, egui::Key::O)
    );
    assert_eq!(
        parse_combo("Ctrl+O").unwrap(),
        (egui::Modifiers::CTRL, egui::Key::O)
    );
    assert_eq!(
        parse_combo("ctrl + shift + p").unwrap(),
        (mods(true, true, false), egui::Key::P)
    );
    assert_eq!(
        parse_combo("Ctrl+Shift+P").unwrap(),
        (mods(true, true, false), egui::Key::P)
    );
    assert_eq!(parse_combo("F5").unwrap().1, egui::Key::F5);
    assert_eq!(parse_combo("Enter").unwrap().1, egui::Key::Enter);
    assert_eq!(
        parse_combo("Super+Space").unwrap().0,
        egui::Modifiers::MAC_CMD
    );
}

#[test]
fn rejects_bad_combos() {
    assert!(parse_combo("").is_err());
    assert!(parse_combo("Ctrl+").is_err());
    assert!(parse_combo("Bogus+O").is_err());
    assert!(parse_combo("Ctrl+NotAKey").is_err());
}

#[test]
fn display_combo_orders_modifiers() {
    let m = mods(true, true, true);
    assert_eq!(display_combo(&m, egui::Key::P), "Ctrl+Shift+Alt+P");
    assert_eq!(display_combo(&egui::Modifiers::NONE, egui::Key::F5), "F5");
    assert_eq!(
        display_combo(&egui::Modifiers::MAC_CMD, egui::Key::Space),
        "Super+Space"
    );
}

#[test]
fn parses_toml_document() {
    let specs = parse_shortcuts(
        r#"
        [[shortcut]]
        key = "Ctrl+O"
        option = "ff"

        [[shortcut]]
        key = "Ctrl+T"
        command = "alacritty"
        args = ["-e", "htop"]

        [[shortcut]]
        key = "Ctrl+N"
        shell = "notify-send hi"
        hint = "Notify"
        "#,
    )
    .unwrap();
    assert_eq!(specs.len(), 3);
    assert_eq!(specs[0].option.as_deref(), Some("ff"));
    assert_eq!(specs[1].args, vec!["-e".to_string(), "htop".to_string()]);
    assert_eq!(specs[2].shell.as_deref(), Some("notify-send hi"));
    assert_eq!(specs[2].hint.as_deref(), Some("Notify"));
}

#[test]
fn empty_document_yields_no_shortcuts() {
    assert!(parse_shortcuts("").unwrap().is_empty());
    assert!(parse_shortcuts("# just a comment\n").unwrap().is_empty());
}

#[test]
fn bad_toml_is_an_error() {
    assert!(parse_shortcuts("[[shortcut]\nkey =").is_err());
    // Right TOML, wrong shape.
    assert!(parse_shortcuts("shortcut = 3").is_err());
}

#[test]
fn compile_builds_each_target_kind() {
    let opt = compile(&ShortcutSpec::option("Ctrl+O", "ff")).unwrap();
    assert!(matches!(opt.target, ShortcutTarget::Option(ref id) if id == "ff"));
    assert_eq!(opt.hint, "Ctrl+O");

    let cmd = compile(&ShortcutSpec::command("Ctrl+T", "alacritty").args(["-e", "htop"])).unwrap();
    match cmd.target {
        ShortcutTarget::Action(SelectAction::Command { program, args }) => {
            assert_eq!(program, "alacritty");
            assert_eq!(args, vec!["-e", "htop"]);
        }
        other => panic!("expected command, got {other:?}"),
    }

    let sh = compile(&ShortcutSpec::shell("Ctrl+N", "notify-send hi")).unwrap();
    assert!(matches!(
        sh.target,
        ShortcutTarget::Action(SelectAction::Shell(_))
    ));

    let hinted = compile(&ShortcutSpec::option("Ctrl+O", "ff").hint("Open")).unwrap();
    assert_eq!(hinted.hint, "Open");
}

#[test]
fn compile_rejects_ambiguous_or_incomplete_specs() {
    let none = ShortcutSpec {
        key: "Ctrl+O".to_string(),
        ..ShortcutSpec::default()
    };
    assert!(compile(&none).is_err());

    let mut both = ShortcutSpec::option("Ctrl+O", "ff");
    both.shell = Some("echo hi".to_string());
    assert!(compile(&both).is_err());

    let mut stray_args = ShortcutSpec::option("Ctrl+O", "ff");
    stray_args.args.push("--nope".to_string());
    assert!(compile(&stray_args).is_err());
}

#[test]
fn compile_all_orders_most_specific_first() {
    let specs = vec![
        ShortcutSpec::option("Ctrl+S", "save"),
        ShortcutSpec::option("Ctrl+Shift+S", "save-as"),
        ShortcutSpec::option("F5", "reload"),
    ];
    let compiled = compile_all(&specs).unwrap();
    assert_eq!(compiled[0].hint, "Ctrl+Shift+S");
    assert_eq!(compiled[1].hint, "Ctrl+S");
    assert_eq!(compiled[2].hint, "F5");
}

#[test]
fn expand_tilde_uses_home() {
    if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        assert_eq!(
            expand_tilde(Path::new("~/x/y.toml")),
            PathBuf::from(&home).join("x/y.toml")
        );
        assert_eq!(expand_tilde(Path::new("~")), PathBuf::from(&home));
    }
    // Non-tilde paths are untouched.
    assert_eq!(
        expand_tilde(Path::new("/tmp/a.toml")),
        PathBuf::from("/tmp/a.toml")
    );
    assert_eq!(
        expand_tilde(Path::new("rel/a.toml")),
        PathBuf::from("rel/a.toml")
    );
}

#[test]
fn load_shortcuts_file_reads_and_reports_missing() {
    let dir = std::env::temp_dir().join(format!("r-modals-sc-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("shortcuts.toml");
    std::fs::write(&path, "[[shortcut]]\nkey = \"Ctrl+O\"\noption = \"ff\"\n").unwrap();
    let specs = load_shortcuts_file(&path).unwrap();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].option.as_deref(), Some("ff"));
    assert!(load_shortcuts_file(&dir.join("missing.toml")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
