//! Typed search prefixes: bind the start of the query to an action.
//!
//! A prefix maps the beginning of the search query to an action. When the
//! query begins with a registered prefix and the user confirms with
//! `Enter`, the action runs with the rest of the query (the *remainder*)
//! and the modal closes without returning a pick — exactly like a
//! standalone shortcut action.
//!
//! Prefixes are declared in TOML as a list of `[[prefix]]` tables, each
//! setting exactly one of `command` (+ optional `args`) or `shell`:
//!
//! ```toml
//! [[prefix]]
//! prefix = ">"
//! command = "notify-send"
//! args = ["{query}"]
//!
//! [[prefix]]
//! prefix = "?"
//! shell = "firefox --search {query}"
//! ```
//!
//! The `{query}` token in `args`/`shell` (and in `command`) is replaced by
//! the remainder — the text typed after the prefix, trimmed. An `=` directly
//! after the prefix (optionally surrounded by spaces) delimits a parameter,
//! so `>search=Hello World` passes `Hello World` without the `=`; the usual
//! space-separated remainder keeps working. In `shell` the remainder is
//! inserted shell-quoted, so do not wrap `{query}` in quotes yourself. When
//! an action sets no `{query}`, the remainder is appended instead: as one
//! extra argument for `command`, or as a single shell-quoted word for
//! `shell`. So `>` alone (empty remainder) still runs the bare action.
//!
//! [`crate::Modal::prefixes_file`] loads a file, [`crate::Modal::prefixes_toml`]
//! takes inline TOML, and [`crate::Modal::prefix`] adds one entry in code.
//! Longest prefixes win when several match (`>>` beats `>`).

use crate::SelectAction;
use std::path::{Path, PathBuf};

/// One `[[prefix]]` entry as written in a TOML file.
///
/// Build one in code with [`PrefixSpec::command`] or [`PrefixSpec::shell`].
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PrefixSpec {
    /// The prefix text that activates this action, e.g. `">"`, `"?"`, `":"`.
    pub prefix: String,
    /// Program to spawn when the prefix is confirmed.
    #[serde(default)]
    pub command: Option<String>,
    /// Arguments for `command`. `{query}` is replaced by the remainder (the
    /// text after the prefix, or the parameter after `=`); when no argument
    /// contains it the remainder is appended as one argument.
    #[serde(default)]
    pub args: Vec<String>,
    /// Shell one-liner to run when the prefix is confirmed
    /// (`sh -c` on unix, `cmd /C` on Windows).
    #[serde(default)]
    pub shell: Option<String>,
    /// Label used for the resolved selection and the on-screen hint.
    /// Defaults to the prefix itself.
    #[serde(default)]
    pub hint: Option<String>,
}

impl PrefixSpec {
    /// Bind `prefix` to a detached program launch.
    pub fn command(prefix: impl Into<String>, program: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            command: Some(program.into()),
            ..Self::default()
        }
    }

    /// Bind `prefix` to a detached shell one-liner.
    pub fn shell(prefix: impl Into<String>, cmdline: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            shell: Some(cmdline.into()),
            ..Self::default()
        }
    }

    /// Append one argument (for [`PrefixSpec::command`]).
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Append many arguments (for [`PrefixSpec::command`]).
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Override the label used for the selection / hint (defaults to the
    /// prefix itself).
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Top-level TOML document: a list of `[[prefix]]` tables.
#[derive(Debug, Clone, Default, serde::Deserialize)]
struct PrefixFile {
    #[serde(default, rename = "prefix")]
    prefixes: Vec<PrefixSpec>,
}

/// Parse a prefixes TOML document.
///
/// Errors carry the TOML parse message or the offending entry.
pub fn parse_prefixes(toml_src: &str) -> Result<Vec<PrefixSpec>, String> {
    let doc: PrefixFile =
        toml::from_str(toml_src).map_err(|e| format!("invalid prefixes TOML: {e}"))?;
    Ok(doc.prefixes)
}

/// Read and parse a prefixes TOML file. A leading `~/` is expanded to
/// `$HOME`, so config paths written the usual way just work.
pub fn load_prefixes_file(path: &Path) -> Result<Vec<PrefixSpec>, String> {
    let path = crate::shortcut::expand_tilde(path);
    let src = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read prefixes file {}: {e}", path.display()))?;
    parse_prefixes(&src)
}

/// Default prefixes file location, following the XDG base directory spec:
/// `$XDG_CONFIG_HOME/r-modals/prefixes.toml`, falling back to
/// `~/.config/r-modals/prefixes.toml` (or `%APPDATA%` on Windows).
///
/// The file is only read when it exists (see [`crate::Modal::auto_prefixes`]).
pub fn default_prefixes_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(dir).join("r-modals").join("prefixes.toml"));
    }
    #[cfg(windows)]
    if let Some(dir) = std::env::var_os("APPDATA").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(dir).join("r-modals").join("prefixes.toml"));
    }
    std::env::var_os("HOME")
        .filter(|s| !s.is_empty())
        .map(|home| {
            PathBuf::from(home)
                .join(".config")
                .join("r-modals")
                .join("prefixes.toml")
        })
}

/// A validated prefix ready to match against the query.
#[derive(Debug, Clone)]
pub(crate) struct CompiledPrefix {
    /// Prefix text (trimmed, non-empty).
    pub(crate) prefix: String,
    /// Label shown in hints and handed to the action.
    pub(crate) hint: String,
    /// What runs when the prefix is confirmed.
    pub(crate) action: SelectAction,
}

/// Where the modal's prefixes come from before compilation.
#[derive(Debug, Clone)]
pub(crate) enum PrefixSource {
    File(PathBuf),
    Toml(String),
    Spec(PrefixSpec),
    /// A prefix already bound to an action (used for callback prefixes, which
    /// cannot be expressed in TOML).
    Compiled(CompiledPrefix),
}

/// Validate one spec and turn it into a compiled prefix.
pub(crate) fn compile(spec: &PrefixSpec) -> Result<CompiledPrefix, String> {
    let prefix = spec.prefix.trim();
    if prefix.is_empty() {
        return Err("prefix entry has an empty prefix".to_string());
    }
    let action = match (spec.command.as_deref(), spec.shell.as_deref()) {
        (Some(prog), None) => SelectAction::Command {
            program: substitute_exe(prog, false),
            args: spec.args.iter().map(|a| substitute_exe(a, false)).collect(),
        },
        (None, Some(cmd)) => SelectAction::Shell(substitute_exe(cmd, true)),
        (None, None) => return Err(format!("prefix {prefix:?} sets neither command nor shell")),
        (Some(_), Some(_)) => return Err(format!("prefix {prefix:?} sets both command and shell")),
    };
    if spec.command.is_none() && !spec.args.is_empty() {
        return Err(format!("prefix {prefix:?} sets args without a command"));
    }
    let hint = spec.hint.clone().unwrap_or_else(|| prefix.to_string());
    Ok(CompiledPrefix {
        prefix: prefix.to_string(),
        hint,
        action,
    })
}

/// Compile every spec, longest prefix first so `>>` is tried before `>`.
pub(crate) fn compile_all(specs: &[PrefixSpec]) -> Result<Vec<CompiledPrefix>, String> {
    let mut out: Vec<CompiledPrefix> = specs.iter().map(compile).collect::<Result<Vec<_>, _>>()?;
    sort_longest_first(&mut out);
    Ok(out)
}

/// Sort prefixes longest-first (stable) so `>>` is matched before `>`.
pub(crate) fn sort_longest_first(out: &mut [CompiledPrefix]) {
    out.sort_by_key(|p| std::cmp::Reverse(p.prefix.len()));
}

/// Find the registered prefix closest to what the user is typing, for Tab
/// completion.
///
/// A candidate starts with the query (letter by letter, via `starts_with`).
/// An exact match is not a completion, and an empty query never completes.
/// The shortest candidate wins, so the completion adds as few characters as
/// possible; ties keep the registered order.
pub(crate) fn completion<'a>(
    prefixes: &'a [CompiledPrefix],
    query: &str,
) -> Option<&'a CompiledPrefix> {
    let q = query.trim_start();
    if q.is_empty() {
        return None;
    }
    prefixes
        .iter()
        .filter(|p| p.prefix != q && p.prefix.starts_with(q))
        .min_by_key(|p| p.prefix.len())
}

/// Find the prefix a query starts with, returning it and the trimmed
/// remainder. Longest matching prefix wins; leading whitespace on the query
/// is ignored. An `=` right after the prefix (spaces allowed around it)
/// separates a parameter from the prefix and is not part of the remainder,
/// so `>search=Hello World` yields `Hello World`. `None` when no prefix
/// matches.
pub(crate) fn match_prefix<'a>(
    prefixes: &'a [CompiledPrefix],
    query: &str,
) -> Option<(&'a CompiledPrefix, String)> {
    let q = query.trim_start();
    let mut best: Option<(&CompiledPrefix, String)> = None;
    for p in prefixes {
        if let Some(rest) = q.strip_prefix(&p.prefix) {
            let longer = match &best {
                Some((b, _)) => p.prefix.len() > b.prefix.len(),
                None => true,
            };
            if longer {
                best = Some((p, parameter(rest)));
            }
        }
    }
    best
}

/// Trim the text after a matched prefix and drop an optional `=` separator.
/// Whitespace before and after the `=` is ignored; without one the rest is
/// the plain space-separated remainder.
fn parameter(rest: &str) -> String {
    let rest = rest.trim_start();
    match rest.strip_prefix('=') {
        Some(param) => param.trim().to_string(),
        None => rest.trim().to_string(),
    }
}

/// Quote one word for the platform shell used by [`SelectAction::Shell`].
pub(crate) fn shell_quote(s: &str) -> String {
    if cfg!(windows) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// The executable that owns the compiled config. Prefixes and shortcuts are
/// compiled in the consumer's dialog child, so `{exe}` expands to that consumer
/// (for example `quick-bitwarden`) rather than to the r-modals library.
pub(crate) fn current_exe() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "r-modals".to_string())
}

/// Replace `{exe}` with the current executable, shell-quoted when the token sits
/// in a shell one-liner and raw in `command`/`args`.
pub(crate) fn substitute_exe(text: &str, quote: bool) -> String {
    if !text.contains("{exe}") {
        return text.to_string();
    }
    let exe = current_exe();
    let repl = if quote { shell_quote(&exe) } else { exe };
    text.replace("{exe}", &repl)
}

/// Replace every `{query}` token in `text` with `rest` (shell-quoted when
/// `quote`). Text without the token is returned unchanged.
fn substitute(text: &str, rest: &str, quote: bool) -> String {
    if !text.contains("{query}") {
        return text.to_string();
    }
    let repl = if quote {
        shell_quote(rest)
    } else {
        rest.to_string()
    };
    text.replace("{query}", &repl)
}

/// Resolve an action against the remainder: `{query}` is substituted, and
/// when an action sets no placeholder the remainder is appended (extra
/// argument for `command`, shell-quoted word for `shell`). Callbacks are
/// returned as-is — they read the remainder from the selection's `query`.
pub(crate) fn resolve_action(action: &SelectAction, rest: &str) -> SelectAction {
    match action {
        SelectAction::Command { program, args } => {
            let placeholder =
                program.contains("{query}") || args.iter().any(|a| a.contains("{query}"));
            let program = substitute(program, rest, false);
            let mut resolved: Vec<String> =
                args.iter().map(|a| substitute(a, rest, false)).collect();
            if !placeholder && !rest.is_empty() {
                resolved.push(rest.to_string());
            }
            SelectAction::Command {
                program,
                args: resolved,
            }
        }
        SelectAction::Shell(cmd) => {
            if cmd.contains("{query}") {
                SelectAction::Shell(substitute(cmd, rest, true))
            } else if rest.is_empty() {
                SelectAction::Shell(cmd.clone())
            } else {
                SelectAction::Shell(format!("{cmd} {}", shell_quote(rest)))
            }
        }
        SelectAction::Callback(_) => action.clone(),
    }
}

impl CompiledPrefix {
    /// Fire this prefix's action with the query remainder. `sel.query` holds
    /// the remainder for callbacks; `sel.label` is the prefix's hint.
    pub(crate) fn execute(&self, sel: &crate::ModalSelection, rest: &str) -> std::io::Result<()> {
        resolve_action(&self.action, rest).execute(sel)
    }
}

#[cfg(test)]
mod tests;
