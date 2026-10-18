//! Unit tests for [`super`].

use super::*;

fn compiled(prefix: &str, action: SelectAction) -> CompiledPrefix {
    CompiledPrefix {
        prefix: prefix.to_string(),
        hint: prefix.to_string(),
        action,
    }
}

fn shell(cmd: &str) -> SelectAction {
    SelectAction::Shell(cmd.to_string())
}

#[test]
fn parses_toml_document() {
    let specs = parse_prefixes(
        r#"
        [[prefix]]
        prefix = ">"
        command = "notify-send"
        args = ["r-modals", "{query}"]

        [[prefix]]
        prefix = "?"
        shell = "firefox --search {query}"
        hint = "Web search"
        "#,
    )
    .unwrap();
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].prefix, ">");
    assert_eq!(specs[0].command.as_deref(), Some("notify-send"));
    assert_eq!(specs[0].args, vec!["r-modals", "{query}"]);
    assert_eq!(specs[1].shell.as_deref(), Some("firefox --search {query}"));
    assert_eq!(specs[1].hint.as_deref(), Some("Web search"));
}

#[test]
fn empty_document_yields_no_prefixes() {
    assert!(parse_prefixes("").unwrap().is_empty());
    assert!(parse_prefixes("# just a comment\n").unwrap().is_empty());
}

#[test]
fn bad_toml_is_an_error() {
    assert!(parse_prefixes("[[prefix]\nprefix =").is_err());
    // Right TOML, wrong shape.
    assert!(parse_prefixes("prefix = 3").is_err());
}

#[test]
fn compile_builds_each_target_kind() {
    let cmd = compile(&PrefixSpec::command(">", "notify-send").args(["-a", "r-modals"])).unwrap();
    assert_eq!(cmd.prefix, ">");
    assert_eq!(cmd.hint, ">");
    match cmd.action {
        SelectAction::Command { program, args } => {
            assert_eq!(program, "notify-send");
            assert_eq!(args, vec!["-a", "r-modals"]);
        }
        other => panic!("expected command, got {other:?}"),
    }

    let sh = compile(&PrefixSpec::shell("?", "firefox --search {query}").hint("Web")).unwrap();
    assert_eq!(sh.hint, "Web");
    assert!(matches!(sh.action, SelectAction::Shell(_)));
}

#[test]
fn compile_rejects_invalid_specs() {
    // Empty / whitespace-only prefix.
    assert!(compile(&PrefixSpec::command("", "true")).is_err());
    assert!(compile(&PrefixSpec::command("   ", "true")).is_err());
    // Neither command nor shell.
    let none = PrefixSpec {
        prefix: ">".to_string(),
        ..PrefixSpec::default()
    };
    assert!(compile(&none).is_err());
    // Both.
    let mut both = PrefixSpec::command(">", "true");
    both.shell = Some("true".to_string());
    assert!(compile(&both).is_err());
    // Stray args without a command.
    let mut stray = PrefixSpec::shell(">", "true");
    stray.args.push("--nope".to_string());
    assert!(compile(&stray).is_err());
}

#[test]
fn compile_all_orders_longest_prefix_first() {
    let specs = vec![
        PrefixSpec::shell(">", "true"),
        PrefixSpec::shell(">>", "true"),
        PrefixSpec::shell(":", "true"),
    ];
    let compiled = compile_all(&specs).unwrap();
    assert_eq!(compiled[0].prefix, ">>");
    assert_eq!(compiled[1].prefix, ">");
    assert_eq!(compiled[2].prefix, ":");
}

#[test]
fn match_prefix_longest_wins_and_remainder_is_trimmed() {
    let prefixes = vec![compiled(">", shell("true")), compiled(">>", shell("true"))];
    let (p, rest) = match_prefix(&prefixes, ">>  ls -la  ").unwrap();
    assert_eq!(p.prefix, ">>");
    assert_eq!(rest, "ls -la");

    // Leading whitespace on the query is ignored.
    let (p, rest) = match_prefix(&prefixes, "   > hi").unwrap();
    assert_eq!(p.prefix, ">");
    assert_eq!(rest, "hi");

    // Prefix alone: empty remainder still matches (runs the bare action).
    let (p, rest) = match_prefix(&prefixes, ">").unwrap();
    assert_eq!(p.prefix, ">");
    assert!(rest.is_empty());
}

#[test]
fn match_prefix_without_match_is_none() {
    let prefixes = vec![compiled(">", shell("true"))];
    assert!(match_prefix(&prefixes, "plain query").is_none());
    assert!(match_prefix(&prefixes, "").is_none());
    assert!(match_prefix(&[], "> hi").is_none());
}

#[test]
fn resolve_action_substitutes_placeholder() {
    let action = SelectAction::Command {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), "echo {query}".to_string()],
    };
    match resolve_action(&action, "hello world") {
        SelectAction::Command { program, args } => {
            assert_eq!(program, "sh");
            assert_eq!(args, vec!["-c".to_string(), "echo hello world".to_string()]);
        }
        other => panic!("expected command, got {other:?}"),
    }

    // Shell placeholder is single-quoted (one word, no splitting).
    let sh = resolve_action(&shell("notify-send {query}"), "hi there");
    assert!(matches!(sh, SelectAction::Shell(c) if c == "notify-send 'hi there'"));
}

#[test]
fn resolve_action_appends_without_placeholder() {
    // Command: remainder becomes one extra argument.
    let action = SelectAction::Command {
        program: "notify-send".to_string(),
        args: vec!["-a".to_string(), "r-modals".to_string()],
    };
    match resolve_action(&action, "build done") {
        SelectAction::Command { args, .. } => {
            assert_eq!(args, vec!["-a", "r-modals", "build done"]);
        }
        other => panic!("expected command, got {other:?}"),
    }

    // Empty remainder leaves the action untouched.
    match resolve_action(&action, "") {
        SelectAction::Command { args, .. } => assert_eq!(args, vec!["-a", "r-modals"]),
        other => panic!("expected command, got {other:?}"),
    }

    // Shell: remainder appended as one quoted word.
    let sh = resolve_action(&shell("notify-send"), "hi there");
    assert!(matches!(sh, SelectAction::Shell(c) if c == "notify-send 'hi there'"));
    // Single quotes are escaped so the word stays one shell argument.
    let sh = resolve_action(&shell("notify-send"), "it's ok");
    assert!(matches!(sh, SelectAction::Shell(c) if c == "notify-send 'it'\\''s ok'"));
}

#[test]
fn resolve_action_leaves_callbacks_untouched() {
    let cb = SelectAction::Callback(std::sync::Arc::new(|_| {}));
    assert!(matches!(
        resolve_action(&cb, "x"),
        SelectAction::Callback(_)
    ));
}

#[test]
fn load_prefixes_file_reads_and_reports_missing() {
    let dir = std::env::temp_dir().join(format!("r-modals-pf-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("prefixes.toml");
    std::fs::write(&path, "[[prefix]]\nprefix = \">\"\nshell = \"true\"\n").unwrap();
    let specs = load_prefixes_file(&path).unwrap();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].prefix, ">");
    assert!(load_prefixes_file(&dir.join("missing.toml")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
