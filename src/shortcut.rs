//! File-configurable keyboard shortcuts.
//!
//! Shortcuts are declared in TOML as a list of `[[shortcut]]` tables. Each
//! entry binds a key combo to one of:
//!
//! - `option = "<id>"` — activate the modal option with that id (drill into a
//!   submenu, or pick a leaf and fire its action);
//! - `command = "<program>"` (+ optional `args = [..]`) — spawn a detached
//!   terminal command;
//! - `shell = "<one-liner>"` — run a detached shell command.
//!
//! ```toml
//! [[shortcut]]
//! key = "Ctrl+O"
//! option = "firefox"
//!
//! [[shortcut]]
//! key = "Ctrl+Shift+T"
//! command = "alacritty"
//! args = ["-e", "htop"]
//!
//! [[shortcut]]
//! key = "Ctrl+N"
//! shell = "notify-send 'hi'"
//! ```
//!
//! `Ctrl`/`Control`, `Shift`, `Alt`/`Option`/`Opt` and
//! `Super`/`Cmd`/`Command`/`Meta`/`Win` are recognised modifiers; the final
//! `+`-separated token is the key (anything [`egui::Key::from_name`] accepts:
//! `O`, `F5`, `Enter`, `Space`, `PageDown`, …).
//!
//! Load one with [`crate::Modal::shortcuts_file`], inline TOML with
//! [`crate::Modal::shortcuts_toml`], or programmatically with
//! [`crate::Modal::shortcut`].

use crate::SelectAction;
use std::path::{Path, PathBuf};

/// One `[[shortcut]]` entry as written in a TOML file.
///
/// Build one in code with [`ShortcutSpec::option`], [`ShortcutSpec::command`]
/// or [`ShortcutSpec::shell`].
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ShortcutSpec {
    /// Key combo, e.g. `"Ctrl+O"`, `"Ctrl+Shift+P"`, `"F5"`.
    pub key: String,
    /// Id of an option in the modal to activate when pressed.
    #[serde(default)]
    pub option: Option<String>,
    /// Program to spawn (with `args`) when pressed.
    #[serde(default)]
    pub command: Option<String>,
    /// Arguments for `command`.
    #[serde(default)]
    pub args: Vec<String>,
    /// Shell one-liner to run when pressed (`sh -c` / `cmd /C`).
    #[serde(default)]
    pub shell: Option<String>,
    /// Optional override for the hint rendered on the bound row. When
    /// omitted the key combo itself is shown.
    #[serde(default)]
    pub hint: Option<String>,
}

impl ShortcutSpec {
    /// Bind `key` to an existing option id.
    pub fn option(key: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            option: Some(id.into()),
            ..Self::default()
        }
    }

    /// Bind `key` to a detached program launch.
    pub fn command(key: impl Into<String>, program: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            command: Some(program.into()),
            ..Self::default()
        }
    }

    /// Bind `key` to a detached shell one-liner.
    pub fn shell(key: impl Into<String>, cmdline: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            shell: Some(cmdline.into()),
            ..Self::default()
        }
    }

    /// Append one argument (for [`ShortcutSpec::command`]).
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Append many arguments (for [`ShortcutSpec::command`]).
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Override the hint shown on the bound row (defaults to the key combo).
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Top-level TOML document: a list of `[[shortcut]]` tables.
#[derive(Debug, Clone, Default, serde::Deserialize)]
struct ShortcutFile {
    #[serde(default, rename = "shortcut")]
    shortcuts: Vec<ShortcutSpec>,
}

/// Parse a shortcuts TOML document.
///
/// Errors carry the TOML parse message or the offending entry.
pub fn parse_shortcuts(toml_src: &str) -> Result<Vec<ShortcutSpec>, String> {
    let doc: ShortcutFile =
        toml::from_str(toml_src).map_err(|e| format!("invalid shortcuts TOML: {e}"))?;
    Ok(doc.shortcuts)
}

/// Read and parse a shortcuts TOML file. A leading `~/` is expanded to
/// `$HOME`, so config paths written the usual way just work.
pub fn load_shortcuts_file(path: &Path) -> Result<Vec<ShortcutSpec>, String> {
    let path = expand_tilde(path);
    let src = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read shortcuts file {}: {e}", path.display()))?;
    parse_shortcuts(&src)
}

/// Expand a leading `~`/`~/…` to `$HOME`. Other paths are returned as-is.
fn expand_tilde(path: &Path) -> PathBuf {
    let Some(s) = path.to_str() else {
        return path.to_path_buf();
    };
    let rest = match s.strip_prefix("~/") {
        Some(r) => r,
        None if s == "~" => "",
        None => return path.to_path_buf(),
    };
    match std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        Some(home) => PathBuf::from(home).join(rest),
        None => path.to_path_buf(),
    }
}

/// Default shortcuts file location, following the XDG base directory spec:
/// `$XDG_CONFIG_HOME/r-modals/shortcuts.toml`, falling back to
/// `~/.config/r-modals/shortcuts.toml` (or `%APPDATA%` on Windows).
///
/// The file is only read when it exists (see [`crate::Modal::auto_shortcuts`]).
pub fn default_shortcuts_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(dir).join("r-modals").join("shortcuts.toml"));
    }
    #[cfg(windows)]
    if let Some(dir) = std::env::var_os("APPDATA").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(dir).join("r-modals").join("shortcuts.toml"));
    }
    std::env::var_os("HOME")
        .filter(|s| !s.is_empty())
        .map(|home| {
            PathBuf::from(home)
                .join(".config")
                .join("r-modals")
                .join("shortcuts.toml")
        })
}

/// Where a shortcut's action comes from once compiled.
#[derive(Debug, Clone)]
pub(crate) enum ShortcutTarget {
    /// Activate the option with this id (drill or pick).
    Option(String),
    /// Spawn a detached command / shell.
    Action(SelectAction),
}

/// A validated shortcut ready to match against keyboard input.
#[derive(Debug, Clone)]
pub(crate) struct CompiledShortcut {
    /// Required modifiers (matched logically: extra shift/alt are ignored).
    pub(crate) modifiers: egui::Modifiers,
    pub(crate) key: egui::Key,
    /// Text shown as the row hint.
    pub(crate) hint: String,
    pub(crate) target: ShortcutTarget,
}

/// Where the modal's shortcuts come from before compilation.
#[derive(Debug, Clone)]
pub(crate) enum ShortcutSource {
    File(PathBuf),
    Toml(String),
    Spec(ShortcutSpec),
}

/// Number of modifier keys a combo pins down. Used to try the most specific
/// shortcuts first: `consume_key` matches logically, so `Ctrl+Shift+S` must be
/// tested before `Ctrl+S` or the latter would swallow it.
fn modifier_count(m: &egui::Modifiers) -> u8 {
    m.alt as u8 + m.ctrl as u8 + m.shift as u8 + m.mac_cmd as u8
}

/// Canonical display text for a combo, e.g. `"Ctrl+Shift+P"`.
pub(crate) fn display_combo(m: &egui::Modifiers, key: egui::Key) -> String {
    let mut s = String::new();
    if m.ctrl {
        s.push_str("Ctrl+");
    }
    if m.shift {
        s.push_str("Shift+");
    }
    if m.alt {
        s.push_str("Alt+");
    }
    if m.mac_cmd {
        s.push_str("Super+");
    }
    s.push_str(key.name());
    s
}

/// Parse a combo like `"Ctrl+Shift+P"` into egui modifiers + key.
pub(crate) fn parse_combo(spec: &str) -> Result<(egui::Modifiers, egui::Key), String> {
    let parts: Vec<&str> = spec
        .split('+')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return Err("empty key combo".to_string());
    }
    let mut modifiers = egui::Modifiers::NONE;
    let mut key: Option<egui::Key> = None;
    for (i, part) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => {
                modifiers.ctrl = true;
                continue;
            }
            "shift" => {
                modifiers.shift = true;
                continue;
            }
            "alt" | "option" | "opt" => {
                modifiers.alt = true;
                continue;
            }
            "super" | "cmd" | "command" | "meta" | "win" => {
                modifiers.mac_cmd = true;
                continue;
            }
            _ => {}
        }
        if !last {
            return Err(format!("unknown modifier {part:?} in key combo {spec:?}"));
        }
        key = Some(
            egui::Key::from_name(part)
                .ok_or_else(|| format!("unknown key {part:?} in key combo {spec:?}"))?,
        );
    }
    let key = key.ok_or_else(|| format!("no key in combo {spec:?}"))?;
    Ok((modifiers, key))
}

/// Validate one spec and turn it into a compiled shortcut.
pub(crate) fn compile(spec: &ShortcutSpec) -> Result<CompiledShortcut, String> {
    let (modifiers, key) = parse_combo(&spec.key)?;
    let hint = spec
        .hint
        .clone()
        .unwrap_or_else(|| display_combo(&modifiers, key));
    let target = match (
        spec.option.as_deref(),
        spec.command.as_deref(),
        spec.shell.as_deref(),
    ) {
        (Some(id), None, None) => ShortcutTarget::Option(id.to_string()),
        (None, Some(prog), None) => ShortcutTarget::Action(SelectAction::Command {
            program: prog.to_string(),
            args: spec.args.clone(),
        }),
        (None, None, Some(cmd)) => ShortcutTarget::Action(SelectAction::Shell(cmd.to_string())),
        (None, None, None) => {
            return Err(format!(
                "shortcut {} sets none of option/command/shell",
                spec.key
            ))
        }
        _ => {
            return Err(format!(
                "shortcut {} must set exactly one of option/command/shell",
                spec.key
            ))
        }
    };
    if spec.command.is_none() && !spec.args.is_empty() {
        return Err(format!("shortcut {} sets args without a command", spec.key));
    }
    Ok(CompiledShortcut {
        modifiers,
        key,
        hint,
        target,
    })
}

/// Compile every spec, most-specific combos first.
pub(crate) fn compile_all(specs: &[ShortcutSpec]) -> Result<Vec<CompiledShortcut>, String> {
    let mut out: Vec<CompiledShortcut> =
        specs.iter().map(compile).collect::<Result<Vec<_>, _>>()?;
    // Stable sort keeps file order among equally specific combos. Generated
    // ties (e.g. `Ctrl+O` vs `Ctrl+Shift+O`) place the richer combo first.
    out.sort_by_key(|s| std::cmp::Reverse(modifier_count(&s.modifiers)));
    Ok(out)
}

#[cfg(test)]
mod tests {
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

        let cmd =
            compile(&ShortcutSpec::command("Ctrl+T", "alacritty").args(["-e", "htop"])).unwrap();
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
}
