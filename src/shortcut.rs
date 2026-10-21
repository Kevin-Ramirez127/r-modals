//! File-configurable keyboard shortcuts.
//!
//! Shortcuts are declared in TOML as a list of `[[shortcut]]` tables. Each
//! entry binds a key combo to one of:
//!
//! - `option = "<id>"` — activate the modal option with that id (drill into a
//!   submenu, or pick a leaf and fire its action);
//! - `pick = "<tag>"` — pick the row that is currently highlighted, tagging the
//!   resulting [`crate::ModalSelection::shortcut`] with `<tag>` so the caller
//!   can vary what the same selection means (e.g. `Shift+Enter` copies the
//!   username instead of the password);
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
//! key = "Shift+Enter"
//! pick = "username"
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
    /// Pick the currently highlighted row and tag the selection with this
    /// value (see [`crate::ModalSelection::shortcut`]).
    #[serde(default)]
    pub pick: Option<String>,
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

    /// Bind `key` to picking the currently highlighted row, tagged `tag`.
    pub fn pick(key: impl Into<String>, tag: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            pick: Some(tag.into()),
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
pub(crate) fn expand_tilde(path: &Path) -> PathBuf {
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
    /// Pick the highlighted row, tagging the selection with this value.
    Pick(String),
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
        spec.pick.as_deref(),
    ) {
        (Some(id), None, None, None) => ShortcutTarget::Option(id.to_string()),
        (None, None, None, Some(tag)) => ShortcutTarget::Pick(tag.to_string()),
        (None, Some(prog), None, None) => ShortcutTarget::Action(SelectAction::Command {
            program: crate::prefix::substitute_exe(prog, false),
            args: spec
                .args
                .iter()
                .map(|a| crate::prefix::substitute_exe(a, false))
                .collect(),
        }),
        (None, None, Some(cmd), None) => ShortcutTarget::Action(SelectAction::Shell(
            crate::prefix::substitute_exe(cmd, true),
        )),
        (None, None, None, None) => {
            return Err(format!(
                "shortcut {} sets none of option/pick/command/shell",
                spec.key
            ))
        }
        _ => {
            return Err(format!(
                "shortcut {} must set exactly one of option/pick/command/shell",
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
mod tests;
