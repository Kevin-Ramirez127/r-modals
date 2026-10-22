use super::*;
use std::ffi::OsString;

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn selection() -> ModalSelection {
    ModalSelection {
        id: "ff".to_string(),
        label: "Firefox".to_string(),
        index: 2,
        path: vec![2],
        query: "fir".to_string(),
        custom_input: false,
        shortcut: Some("browser".to_string()),
    }
}

#[test]
fn option_lines_split_on_the_first_tab() {
    let opt = parse_option_line("ff\tFirefox\tNightly").unwrap();
    assert_eq!(opt.id, "ff");
    assert_eq!(opt.label, "Firefox\tNightly");

    let opt = parse_option_line("plain").unwrap();
    assert_eq!(opt.id, "plain");
    assert_eq!(opt.label, "plain");

    assert!(parse_option_line("").is_none());
    assert!(parse_option_line("\r").is_none());
}

#[test]
fn output_formats_match_the_cli_contract() {
    let sel = selection();
    assert_eq!(format_selection(&sel, OutputFormat::Label), "Firefox\n");
    assert_eq!(format_selection(&sel, OutputFormat::Id), "ff\n");
    assert_eq!(
        format_selection(&sel, OutputFormat::Tag),
        "browser\nFirefox\n"
    );

    let json = format_selection(&sel, OutputFormat::Json);
    let decoded: ModalSelection = serde_json::from_str(json.trim()).unwrap();
    assert_eq!(decoded.id, "ff");
    assert_eq!(decoded.label, "Firefox");
    assert_eq!(decoded.index, 2);
    assert_eq!(decoded.path, vec![2]);
    assert_eq!(decoded.query, "fir");
    assert_eq!(decoded.shortcut.as_deref(), Some("browser"));
}

#[test]
fn child_flags_are_parsed_into_options() {
    let o = parse_child_opts(&args(&[
        "--prompt",
        "Pick a browser",
        "--format",
        "id",
        "--multi",
        "--fuzzy",
        "--case-sensitive",
        "--allow-custom",
        "--max-results",
        "7",
        "--min-match-chars",
        "2",
        "--no-title",
        "--theme",
        "dark",
    ]))
    .unwrap();

    assert_eq!(o.prompt.as_deref(), Some("Pick a browser"));
    assert_eq!(o.format, OutputFormat::Id);
    assert!(o.multi);
    assert!(o.fuzzy);
    assert!(o.case_sensitive);
    assert!(o.allow_custom);
    assert_eq!(o.max_results, Some(7));
    assert_eq!(o.min_match_chars, Some(2));
    assert_eq!(o.show_title, Some(false));
    assert_eq!(o.theme.as_deref(), Some("dark"));
}

#[test]
fn positional_and_section_sources_are_parsed() {
    let o = parse_child_opts(&args(&[
        "Master password",
        "--password",
        "--initial",
        "hunter2",
        "--message",
        "Unlock the vault",
        "--config",
        "/tmp/config.toml",
        "--no-shortcuts",
    ]))
    .unwrap();

    assert_eq!(o.positional.as_deref(), Some("Master password"));
    assert!(o.password);
    assert_eq!(o.initial.as_deref(), Some("hunter2"));
    assert_eq!(o.message.as_deref(), Some("Unlock the vault"));
    assert_eq!(
        o.config.as_deref(),
        Some(std::path::Path::new("/tmp/config.toml"))
    );
    assert!(o.no_shortcuts);
}

#[test]
fn bad_flags_and_values_are_rejected() {
    assert!(parse_child_opts(&args(&["--nope"])).is_err());
    assert!(parse_child_opts(&args(&["--max-results", "many"])).is_err());
    assert!(parse_child_opts(&args(&["--format", "xml"])).is_err());
    assert!(parse_child_opts(&args(&["one", "two"])).is_err());
    assert!(parse_child_opts(&args(&["--theme"])).is_err());
}

#[test]
fn mode_must_be_the_first_argument() {
    let input = args(&["--menu", "--format", "json"]);
    let (mode, rest) = split_mode(&input).unwrap();
    assert_eq!(mode, Mode::Menu);
    assert_eq!(rest, &args(&["--format", "json"]));

    assert!(split_mode(&args(&["--theme", "dark", "--menu"])).is_none());
    assert!(split_mode(&args(&["--ask", "Name"])).is_some());
    assert!(split_mode(&args(&["--dmenu"])).is_some());
    assert!(split_mode(&args(&["--confirm", "Sure?"])).is_some());
    assert!(split_mode(&args(&["--error", "Oops"])).is_some());
}

#[test]
fn ids_cannot_break_the_line_protocol() {
    assert_eq!(sanitize("a\tb\nc\r", true), "a b c ");
    // Tabs are meaningful in labels and remain after the first separator.
    assert_eq!(sanitize("a\tb\nc", false), "a\tb c");
}

#[test]
fn double_dash_stops_option_parsing() {
    let o = parse_child_opts(&args(&["--password", "--", "--help"])).unwrap();
    assert!(o.password);
    assert_eq!(o.positional.as_deref(), Some("--help"));

    // Without the separator, a leading dash is still rejected as a flag.
    assert!(parse_child_opts(&args(&["--help"])).is_err());
    // A separator on its own is not a positional.
    assert!(parse_child_opts(&args(&["--"]))
        .unwrap()
        .positional
        .is_none());
    // Only one positional is accepted.
    assert!(parse_child_opts(&args(&["--", "one", "two"])).is_err());
}

#[test]
fn control_characters_cannot_reach_the_terminal_or_line_protocol() {
    // ESC/OSC sequences and newlines are neutralised in labels and ids.
    let opt = parse_option_line("i\x1b\tl\x07abel").unwrap();
    assert_eq!(opt.id, "i ");
    assert_eq!(opt.label, "l abel");

    let mut sel = selection();
    sel.label = "evil\x1b[2Jlabel".to_string();
    assert_eq!(
        format_selection(&sel, OutputFormat::Label),
        "evil [2Jlabel\n"
    );
    sel.id = "a\tb\x07".to_string();
    assert_eq!(format_selection(&sel, OutputFormat::Id), "a b \n");
}

#[test]
fn option_lines_are_bounded() {
    let mut options = Vec::new();
    // Exactly the limit (without the newline) is accepted.
    let mut input = vec![b'a'; MAX_OPTION_LINE];
    input.push(b'\n');
    read_options_into(&mut std::io::Cursor::new(input), &mut options).unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].id.len(), MAX_OPTION_LINE);

    // One byte over is rejected without buffering the rest of the line.
    let mut input = vec![b'a'; MAX_OPTION_LINE + 1];
    input.extend_from_slice(b"\nshort\n");
    let mut options = Vec::new();
    assert!(read_options_into(&mut std::io::Cursor::new(input), &mut options).is_err());
}

#[cfg(unix)]
#[test]
fn dialog_positionals_are_passed_after_double_dash() {
    let capture = std::env::temp_dir().join(format!(
        "r-modals-argv-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let (dir, script) = fake_child(
        "argv",
        &format!("printf '%s\\n' \"$@\" > '{}'", capture.display()),
    );

    // A title that looks like a flag must arrive as data, not as an option.
    let output = Dialog::ask("--password").program(&script).run().unwrap();
    assert!(matches!(output, DialogOutput::Text(ref s) if s.is_empty()));

    let argv = std::fs::read_to_string(&capture).unwrap();
    assert_eq!(argv, "--ask\n--\n--password\n");

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&capture);
}

#[test]
fn child_output_is_capped_but_fully_drained() {
    // The reader consumes the whole stream (so the child cannot deadlock on a
    // full pipe) while retaining only the first `limit` bytes.
    let out = read_all_capped(std::io::Cursor::new(vec![b'a'; 10_000]), 16).unwrap();
    assert_eq!(out, vec![b'a'; 16]);
}

#[test]
fn theme_names_and_inline_css_resolve() {
    let (theme, css) = resolve_theme(Some("dark")).unwrap();
    assert_eq!(theme.background, Theme::dark().background);
    assert!(css.is_none());

    let (_, css) = resolve_theme(Some("#fff { color: red; }")).unwrap();
    assert_eq!(css.as_deref(), Some("#fff { color: red; }"));

    let (_, css) = resolve_theme(None).unwrap();
    assert!(css.is_none());
}

#[test]
fn pid_file_is_written_and_removed_on_drop() {
    let dir = std::env::temp_dir().join(format!(
        "r-modals-pid-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = dir.join("dialog.pid");
    {
        let _guard = write_pid_file(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.trim(), std::process::id().to_string());
    }
    assert!(!path.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
fn fake_child(name: &str, body: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;

    let dir = std::env::temp_dir().join(format!(
        "r-modals-dialog-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("child.sh");
    std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, script)
}

#[cfg(unix)]
#[test]
fn menu_session_sends_options_and_parses_json() {
    let capture = std::env::temp_dir().join(format!(
        "r-modals-capture-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let json = r#"{"id":"b","label":"Beta","index":1,"path":[1],"query":"","custom_input":false,"shortcut":null}"#;
    let (dir, script) = fake_child(
        "json",
        &format!("cat > '{}'\nprintf '%s\\n' '{json}'", capture.display()),
    );

    let mut session = Dialog::menu().program(&script).start().unwrap();
    session
        .send_options([("a", "Alpha"), ("b", "Beta")])
        .unwrap();
    let output = session.wait().unwrap();

    match output {
        DialogOutput::Picked(sel) => {
            assert_eq!(sel.id, "b");
            assert_eq!(sel.label, "Beta");
            assert_eq!(sel.index, 1);
        }
        other => panic!("expected Picked, got {other:?}"),
    }
    assert_eq!(
        std::fs::read_to_string(&capture).unwrap(),
        "a\tAlpha\nb\tBeta\n"
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&capture);
}

#[cfg(unix)]
#[test]
fn wait_maps_exit_codes_and_raw_output() {
    let (dir1, cancel) = fake_child("cancel", "exit 1");
    assert!(matches!(
        Dialog::menu().program(&cancel).run().unwrap(),
        DialogOutput::Cancelled
    ));

    let (dir2, error) = fake_child("error", "echo 'r-modals: boom' >&2\nexit 2");
    let err = Dialog::menu().program(&error).run().unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("boom"));
    assert!(!msg.contains("r-modals: r-modals"));

    let (dir3, raw) = fake_child("raw", "printf 'Beta\\n'");
    let output = Dialog::menu().program(&raw).raw_format().run().unwrap();
    assert!(matches!(output, DialogOutput::Raw(ref s) if s == "Beta"));

    let (dir4, ask) = fake_child("ask", "printf 'secret\\n'");
    let output = Dialog::ask("Password").program(&ask).run().unwrap();
    assert!(matches!(output, DialogOutput::Text(ref s) if s == "secret"));

    for dir in [dir1, dir2, dir3, dir4] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
