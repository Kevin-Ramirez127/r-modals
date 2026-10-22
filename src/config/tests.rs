use super::*;

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "r-modals-config-{name}-{}-{nanos}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const TEMPLATE: &str = r#"
# Header comment; a mention of [[prefix]] is not a section.
[[prefix]]
prefix = ">sync"
shell = "{exe} sync"

[[prefix]]
prefix = ">lock"
shell = "{exe} lock"

[[shortcut]]
key = "Ctrl+K"
copy = "username"

[[shortcut]]
key = "Ctrl+S"
copy = "password"
"#;

#[test]
fn fresh_file_uses_template_and_substitutes_vars() {
    let dir = temp_dir("fresh");
    let path = dir.join("config.toml");
    let config = ConfigFile::at(&path)
        .template(TEMPLATE)
        .var("exe", "/opt/quick-bitwarden");

    assert!(config.ensure().unwrap());
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("shell = \"/opt/quick-bitwarden sync\""));
    assert!(text.contains("shell = \"/opt/quick-bitwarden lock\""));
    assert!(!text.contains("{exe}"));
    assert!(parse_prefixes(&text).unwrap().len() == 2);
    assert!(parse_shortcuts(&text).unwrap().len() == 2);

    // Idempotent: everything from the template already exists.
    assert!(!config.ensure().unwrap());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn builtins_are_appended_once_without_touching_user_entries() {
    let dir = temp_dir("append");
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        r#"
[[prefix]]
prefix = ">mine"
shell = "echo mine"
"#,
    )
    .unwrap();

    let config = ConfigFile::at(&path).template(TEMPLATE).var("exe", "qbw");
    assert!(config.ensure().unwrap());
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(">mine"));
    assert_eq!(text.matches("prefix = \">sync\"").count(), 1);
    assert_eq!(text.matches("prefix = \">lock\"").count(), 1);
    assert_eq!(text.matches("key = \"Ctrl+K\"").count(), 1);
    assert_eq!(text.matches("key = \"Ctrl+S\"").count(), 1);
    assert!(!config.ensure().unwrap());
}

#[test]
fn legacy_files_are_merged_then_removed() {
    let dir = temp_dir("legacy");
    let path = dir.join("config.toml");
    let prefixes = dir.join("prefixes.toml");
    let shortcuts = dir.join("shortcuts.toml");
    std::fs::write(
        &prefixes,
        "[[prefix]]\nprefix = \">old\"\nshell = \"echo old\"\n",
    )
    .unwrap();
    std::fs::write(
        &shortcuts,
        "[[shortcut]]\nkey = \"Ctrl+O\"\ncopy = \"username\"\n",
    )
    .unwrap();

    let config = ConfigFile::at(&path)
        .template(TEMPLATE)
        .migrate_from([&prefixes, &shortcuts]);
    assert!(config.ensure().unwrap());
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(">old"));
    assert!(text.contains("Ctrl+O"));
    assert!(!prefixes.exists());
    assert!(!shortcuts.exists());
}

#[test]
fn malformed_file_is_left_alone() {
    let dir = temp_dir("malformed");
    let path = dir.join("config.toml");
    let broken = "this is not = valid toml\n[[prefix]\n";
    std::fs::write(&path, broken).unwrap();

    let config = ConfigFile::at(&path).template(TEMPLATE).var("exe", "qbw");
    assert!(!config.ensure().unwrap());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
}

#[test]
fn an_existing_file_wins_over_the_legacy_files() {
    let dir = temp_dir("existing");
    let path = dir.join("config.toml");
    let legacy = dir.join("prefixes.toml");
    std::fs::write(&path, "[[prefix]]\nprefix = \">new\"\nshell = \"true\"\n").unwrap();
    std::fs::write(
        &legacy,
        "[[prefix]]\nprefix = \">legacy\"\nshell = \"true\"\n",
    )
    .unwrap();

    let config = ConfigFile::at(&path).migrate_from([&legacy]);
    assert!(!config.ensure().unwrap());
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(">new"));
    assert!(!text.contains(">legacy"));
    // Legacy files are only consumed when the target had to be seeded.
    assert!(legacy.exists());
}

#[test]
fn template_sections_without_a_known_key_are_skipped() {
    let dir = temp_dir("unknown-block");
    let path = dir.join("config.toml");
    std::fs::write(&path, "[[prefix]]\nprefix = \">kept\"\nshell = \"true\"\n").unwrap();
    let template = r#"
[[theme]]
name = "ignored"

[[prefix]]
prefix = ">new"
shell = "true"
"#;
    let config = ConfigFile::at(&path).template(template);
    assert!(config.ensure().unwrap());
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(">kept"));
    assert!(text.contains(">new"));
    assert!(!text.contains("ignored"));
}
