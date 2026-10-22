//! Spawnable dialogs: a stable CLI contract plus a parent-side session API.
//!
//! A consumer binary opts in once:
//!
//! ```no_run
//! r_modals::dialog_main!();
//! // ...the app's own commands...
//! ```
//!
//! and can then spawn itself:
//!
//! ```no_run
//! use r_modals::{Dialog, DialogOutput};
//!
//! let mut picker = Dialog::menu().start().unwrap();
//! picker.send_options([("ff", "Firefox"), ("term", "Terminal")]).unwrap();
//! if let Ok(DialogOutput::Picked(sel)) = picker.wait() {
//!     println!("picked {}", sel.label);
//! }
//! ```
//!
//! The same modes are available from the `r-modals` binary, so shell scripts
//! get the identical contract without a Rust parent:
//!
//! ```text
//! printf 'ff\tFirefox\nterm\tTerminal\n' | r-modals --menu --format json
//! r-modals --ask "Master password" --password
//! r-modals --confirm "Delete?" --message "This cannot be undone."
//! r-modals --error "Something went wrong"
//! ```
//!
//! Protocol: stdin is one option per line (`label` or `id<TAB>label`); stdout
//! is the selection in the requested format; exit codes are `0` pick, `1`
//! cancel, `2` error.

use std::ffi::OsString;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

use crate::{Confirm, Modal, ModalError, ModalOption, ModalSelection, Prompt, Theme};

/// What a finished dialog produced.
#[derive(Debug, Clone)]
pub enum DialogOutput {
    /// The menu selection (full metadata, from the JSON protocol).
    Picked(ModalSelection),
    /// Every pick from a `multi` menu.
    PickedMany(Vec<ModalSelection>),
    /// Text typed into a prompt.
    Text(String),
    /// `true` for confirm, `false` for cancel.
    Confirmed(bool),
    /// An error/info dialog was shown.
    Shown,
    /// Esc, window close, or an external kill.
    Cancelled,
    /// Raw stdout when [`Dialog::raw_format`] was used.
    Raw(String),
}

/// Parent-side builder for one dialog process.
#[derive(Debug, Clone)]
pub struct Dialog {
    kind: Kind,
    program: Option<PathBuf>,
    app_id: Option<String>,
    pid_file: Option<PathBuf>,
    args: Vec<OsString>,
    typed_json: bool,
    multi: bool,
}

#[derive(Debug, Clone)]
enum Kind {
    Menu,
    Ask(String),
    Confirm(String),
    Error(String),
}

impl Dialog {
    /// Line-driven option picker (`--menu`).
    pub fn menu() -> Self {
        Self {
            kind: Kind::Menu,
            program: None,
            app_id: None,
            pid_file: None,
            args: Vec::new(),
            typed_json: true,
            multi: false,
        }
    }

    /// Single-line text prompt (`--ask LABEL`).
    pub fn ask(label: impl Into<String>) -> Self {
        Self::with_kind(Kind::Ask(label.into()))
    }

    /// Yes/no dialog (`--confirm TITLE`).
    pub fn confirm(title: impl Into<String>) -> Self {
        Self::with_kind(Kind::Confirm(title.into()))
    }

    /// One-button error/info dialog (`--error MESSAGE`).
    pub fn error(message: impl Into<String>) -> Self {
        Self::with_kind(Kind::Error(message.into()))
    }

    fn with_kind(kind: Kind) -> Self {
        Self {
            kind,
            program: None,
            app_id: None,
            pid_file: None,
            args: Vec::new(),
            typed_json: false,
            multi: false,
        }
    }

    /// Binary to spawn. Defaults to the current executable, which must call
    /// [`dialog_main!`](crate::dialog_main).
    pub fn program(mut self, exe: impl Into<PathBuf>) -> Self {
        self.program = Some(exe.into());
        self
    }

    /// Wayland app_id / X11 class hint for the window.
    pub fn app_id(mut self, id: impl Into<String>) -> Self {
        self.app_id = Some(id.into());
        self
    }

    /// Let an external process find and kill the window by pid.
    pub fn pid_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.pid_file = Some(path.into());
        self
    }

    /// Append one raw CLI argument.
    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    fn flag(self, flag: &str, value: impl Into<OsString>) -> Self {
        self.arg(flag).arg(value)
    }

    /// One unified config file holding both prefixes and shortcuts.
    pub fn config_file(self, path: impl Into<PathBuf>) -> Self {
        self.flag("--config", path.into())
    }

    /// Load shortcuts from this TOML file.
    pub fn shortcuts_file(self, path: impl Into<PathBuf>) -> Self {
        self.flag("--shortcuts", path.into())
    }

    /// Load query prefixes from this TOML file.
    pub fn prefixes_file(self, path: impl Into<PathBuf>) -> Self {
        self.flag("--prefixes", path.into())
    }

    /// Use a named theme, CSS file, or inline CSS (same as `--theme`).
    pub fn theme(self, spec: impl Into<String>) -> Self {
        self.flag("--theme", spec.into())
    }

    /// Inline CSS stylesheet.
    pub fn stylesheet(self, css: impl Into<String>) -> Self {
        self.flag("--theme", css.into())
    }

    /// Search placeholder.
    pub fn placeholder(self, text: impl Into<String>) -> Self {
        self.flag("--placeholder", text.into())
    }

    /// Message shown while waiting for the options batch.
    pub fn loading_message(self, text: impl Into<String>) -> Self {
        self.flag("--loading-message", text.into())
    }

    /// Hide the full list until the query reaches `chars` characters.
    pub fn min_match_chars(self, chars: usize) -> Self {
        self.flag("--min-match-chars", chars.to_string())
    }

    /// Small hint line at the bottom of the modal.
    pub fn footer_hint(self, hint: impl Into<String>) -> Self {
        self.flag("--footer-hint", hint.into())
    }

    /// Return free-typed text when nothing matches.
    pub fn allow_custom_input(self, yes: bool) -> Self {
        if yes {
            self.arg("--allow-custom")
        } else {
            self
        }
    }

    /// Message shown when the filter matches nothing.
    pub fn empty_message(self, message: impl Into<String>) -> Self {
        self.flag("--empty-message", message.into())
    }

    /// Ranked fuzzy matching.
    pub fn fuzzy(self, yes: bool) -> Self {
        if yes {
            self.arg("--fuzzy")
        } else {
            self
        }
    }

    /// Case-sensitive matching.
    pub fn case_sensitive(self, yes: bool) -> Self {
        if yes {
            self.arg("--case-sensitive")
        } else {
            self
        }
    }

    /// Cap the number of visible rows.
    pub fn max_results(self, n: usize) -> Self {
        self.flag("--max-results", n.to_string())
    }

    /// Space-toggle multi-select for a menu.
    pub fn multi(mut self, yes: bool) -> Self {
        self.multi = yes;
        if yes {
            self = self.arg("--multi");
        }
        self
    }

    /// Read all options before opening instead of showing the loading state.
    pub fn no_stream(self) -> Self {
        self.arg("--no-stream")
    }

    /// Do not request JSON from a menu; [`DialogSession::wait`] then returns
    /// [`DialogOutput::Raw`].
    pub fn raw_format(mut self) -> Self {
        self.typed_json = false;
        self
    }

    /// Mask prompt input.
    pub fn password(self, yes: bool) -> Self {
        if yes {
            self.arg("--password")
        } else {
            self
        }
    }

    /// Pre-filled prompt value.
    pub fn initial(self, value: impl Into<String>) -> Self {
        self.flag("--initial", value.into())
    }

    /// Body text for prompts/dialogs.
    pub fn message(self, text: impl Into<String>) -> Self {
        self.flag("--message", text.into())
    }

    /// Window title for an `--error` dialog (the message is the body).
    pub fn title(self, text: impl Into<String>) -> Self {
        self.flag("--title", text.into())
    }

    /// Confirm button text.
    pub fn confirm_label(self, text: impl Into<String>) -> Self {
        self.flag("--confirm-label", text.into())
    }

    /// Cancel button text.
    pub fn cancel_label(self, text: impl Into<String>) -> Self {
        self.flag("--cancel-label", text.into())
    }

    /// Render the title in the body. Defaults to true.
    pub fn show_title(self, yes: bool) -> Self {
        if yes {
            self
        } else {
            self.arg("--no-title")
        }
    }

    /// Spawn the dialog process without waiting.
    pub fn start(self) -> Result<DialogSession, ModalError> {
        let program = match self.program {
            Some(p) => p,
            None => std::env::current_exe()
                .map_err(|e| ModalError::Io(format!("cannot locate current executable: {e}")))?,
        };
        let mut cmd = Command::new(&program);
        match &self.kind {
            Kind::Menu => {
                cmd.arg("--menu");
            }
            Kind::Ask(label) => {
                cmd.arg("--ask").arg(label);
            }
            Kind::Confirm(title) => {
                cmd.arg("--confirm").arg(title);
            }
            Kind::Error(message) => {
                cmd.arg("--error").arg(message);
            }
        }
        if let Some(id) = &self.app_id {
            cmd.arg("--app-id").arg(id);
        }
        if let Some(pid) = &self.pid_file {
            cmd.arg("--pid-file").arg(pid);
        }
        // Respect a format requested through `.arg(..)`: only the implicit
        // JSON protocol is typed; an explicit label/id output is raw.
        let explicit_format = self
            .args
            .iter()
            .position(|a| a == "--format")
            .and_then(|i| self.args.get(i + 1));
        let has_format = explicit_format.is_some() || self.args.iter().any(|a| a == "--print-id");
        let typed_json = self.typed_json && !has_format;
        let cli_multi = self.args.iter().any(|a| a == "--multi");
        let multi = self.multi || cli_multi;
        if matches!(&self.kind, Kind::Menu) && self.typed_json && !has_format {
            cmd.arg("--format").arg("json");
        }
        cmd.args(&self.args);

        if matches!(&self.kind, Kind::Menu) {
            cmd.stdin(Stdio::piped());
        } else {
            cmd.stdin(Stdio::null());
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| ModalError::Io(format!("cannot spawn {}: {e}", program.display())))?;
        let stdin = child.stdin.take();
        Ok(DialogSession {
            child: Some(child),
            stdin,
            kind: self.kind,
            typed_json,
            multi,
            pid_file: self.pid_file,
        })
    }

    /// Start the dialog, close its input, and wait for the result.
    pub fn run(self) -> Result<DialogOutput, ModalError> {
        let mut session = self.start()?;
        session.close_input();
        session.wait()
    }
}

/// A spawned dialog process.
pub struct DialogSession {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    kind: Kind,
    typed_json: bool,
    multi: bool,
    pid_file: Option<PathBuf>,
}

impl std::fmt::Debug for DialogSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DialogSession")
            .field("pid", &self.pid())
            .field("kind", &self.kind)
            .finish()
    }
}

impl DialogSession {
    /// The child pid, for an external `hide`/`toggle`.
    pub fn pid(&self) -> u32 {
        self.child.as_ref().map(Child::id).unwrap_or(0)
    }

    /// Send `(id, label)` options. Each label becomes one row; the id is what
    /// [`ModalSelection::id`] reports back.
    pub fn send_options<I, S>(&mut self, options: I) -> io::Result<()>
    where
        I: IntoIterator<Item = (S, S)>,
        S: AsRef<str>,
    {
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "dialog input is already closed")
        })?;
        for (id, label) in options {
            let id = sanitize(id.as_ref(), true);
            let label = sanitize(label.as_ref(), false);
            writeln!(stdin, "{id}\t{label}")?;
        }
        stdin.flush()
    }

    /// Send label-only options (`id == label`).
    pub fn send_labels<I, S>(&mut self, labels: I) -> io::Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "dialog input is already closed")
        })?;
        for label in labels {
            writeln!(stdin, "{}", sanitize(label.as_ref(), true))?;
        }
        stdin.flush()
    }

    /// Close the option stream. The child keeps the list it already received.
    pub fn close_input(&mut self) {
        drop(self.stdin.take());
    }

    /// Kill the child and reap it. Safe to call after the window was closed.
    pub fn cancel(mut self) -> io::Result<()> {
        self.close_input();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.remove_pid();
        Ok(())
    }

    /// Wait for the pick/cancel/error and decode the result.
    pub fn wait(mut self) -> Result<DialogOutput, ModalError> {
        self.close_input();
        let Some(child) = self.child.take() else {
            return Err(ModalError::Protocol("dialog was already waited on".into()));
        };
        let out = child
            .wait_with_output()
            .map_err(|e| ModalError::Io(format!("dialog wait failed: {e}")))?;
        self.remove_pid();

        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        match out.status.code() {
            Some(0) => match &self.kind {
                Kind::Menu => self.parse_menu(&stdout),
                Kind::Ask(_) => Ok(DialogOutput::Text(
                    stdout.trim_end_matches(['\n', '\r']).to_string(),
                )),
                Kind::Confirm(_) => Ok(DialogOutput::Confirmed(true)),
                Kind::Error(_) => Ok(DialogOutput::Shown),
            },
            Some(1) => {
                if matches!(&self.kind, Kind::Confirm(_)) {
                    Ok(DialogOutput::Confirmed(false))
                } else {
                    Ok(DialogOutput::Cancelled)
                }
            }
            Some(2) => {
                let mut msg = stderr.trim();
                // The child already prefixes its diagnostics. Keep the wrapper
                // from producing `r-modals backend error: r-modals: ...`.
                for prefix in [
                    "r-modals: ",
                    "r-modals backend error: ",
                    "r-modals shortcuts error: ",
                    "r-modals prefixes error: ",
                    "r-modals CSS error: ",
                ] {
                    if let Some(rest) = msg.strip_prefix(prefix) {
                        msg = rest;
                    }
                }
                Err(ModalError::Backend(if msg.is_empty() {
                    "dialog failed".to_string()
                } else {
                    msg.to_string()
                }))
            }
            Some(code) => Err(ModalError::Protocol(format!(
                "dialog exited with status {code}"
            ))),
            None => Ok(DialogOutput::Cancelled),
        }
    }

    fn parse_menu(&self, stdout: &str) -> Result<DialogOutput, ModalError> {
        if !self.typed_json {
            return Ok(DialogOutput::Raw(
                stdout.trim_end_matches(['\n', '\r']).to_string(),
            ));
        }
        let text = stdout.trim();
        if text.is_empty() {
            return Err(ModalError::Protocol("menu printed no selection".into()));
        }
        if self.multi {
            let mut picks = Vec::new();
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                picks.push(
                    serde_json::from_str::<ModalSelection>(line).map_err(|e| {
                        ModalError::Protocol(format!("invalid selection JSON: {e}"))
                    })?,
                );
            }
            Ok(DialogOutput::PickedMany(picks))
        } else {
            let sel = serde_json::from_str::<ModalSelection>(text)
                .map_err(|e| ModalError::Protocol(format!("invalid selection JSON: {e}")))?;
            Ok(DialogOutput::Picked(sel))
        }
    }

    fn remove_pid(&self) {
        if let Some(path) = &self.pid_file {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Drop for DialogSession {
    fn drop(&mut self) {
        self.close_input();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.remove_pid();
    }
}

/// Replace characters that would break the line protocol.
fn sanitize(text: &str, is_id: bool) -> String {
    text.chars()
        .map(|c| match c {
            '\n' | '\r' => ' ',
            '\t' if is_id => ' ',
            _ => c,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Child side
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Menu,
    Ask,
    Confirm,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum OutputFormat {
    #[default]
    Label,
    Id,
    Tag,
    Json,
}

#[derive(Debug, Clone)]
struct ChildOpts {
    prompt: Option<String>,
    placeholder: Option<String>,
    layout: Option<String>,
    theme: Option<String>,
    multi: bool,
    fuzzy: bool,
    case_sensitive: bool,
    allow_custom: bool,
    history_file: Option<String>,
    empty_message: Option<String>,
    max_results: Option<usize>,
    format: OutputFormat,
    shortcuts: Option<PathBuf>,
    no_shortcuts: bool,
    prefixes: Option<PathBuf>,
    no_prefixes: bool,
    config: Option<PathBuf>,
    app_id: String,
    pid_file: Option<PathBuf>,
    loading_message: Option<String>,
    min_match_chars: Option<usize>,
    footer_hint: Option<String>,
    no_stream: bool,
    message: Option<String>,
    initial: Option<String>,
    password: bool,
    confirm_label: Option<String>,
    cancel_label: Option<String>,
    show_title: Option<bool>,
    title: Option<String>,
    positional: Option<String>,
}

impl Default for ChildOpts {
    fn default() -> Self {
        Self {
            prompt: None,
            placeholder: None,
            layout: None,
            theme: None,
            multi: false,
            fuzzy: false,
            case_sensitive: false,
            allow_custom: false,
            history_file: None,
            empty_message: None,
            max_results: None,
            format: OutputFormat::Label,
            shortcuts: None,
            no_shortcuts: false,
            prefixes: None,
            no_prefixes: false,
            config: None,
            app_id: "r-modals".to_string(),
            pid_file: None,
            loading_message: None,
            min_match_chars: None,
            footer_hint: None,
            no_stream: false,
            message: None,
            initial: None,
            password: false,
            confirm_label: None,
            cancel_label: None,
            show_title: None,
            title: None,
            positional: None,
        }
    }
}

/// Dispatch a dialog mode from `std::env::args` and exit with its code.
///
/// Call this first in `main` (or use [`dialog_main!`](crate::dialog_main)).
/// Returns `None` when the first argument is not a dialog mode, so the app can
/// continue with its own command parsing.
pub fn run_child_from_args() -> Option<i32> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    run_child_args(&args)
}

/// Dispatch a dialog mode from an explicit argument list.
///
/// The mode must be `args[0]` (`--menu`/`--dmenu`/`--stdin`, `--ask`,
/// `--confirm`, or `--error`). Returns `None` when it is not a dialog mode.
pub fn run_child_args(args: &[OsString]) -> Option<i32> {
    let (mode, rest) = split_mode(args)?;
    let opts = match parse_child_opts(rest) {
        Ok(opts) => opts,
        Err(e) => {
            eprintln!("r-modals: {e}");
            return Some(2);
        }
    };
    Some(run_child_mode(mode, opts))
}

fn split_mode(args: &[OsString]) -> Option<(Mode, &[OsString])> {
    let first = args.first()?.to_string_lossy();
    let mode = match first.as_ref() {
        "--menu" | "--dmenu" | "--stdin" => Mode::Menu,
        "--ask" => Mode::Ask,
        "--confirm" => Mode::Confirm,
        "--error" => Mode::Error,
        _ => return None,
    };
    Some((mode, &args[1..]))
}

fn parse_child_opts(args: &[OsString]) -> Result<ChildOpts, String> {
    let mut o = ChildOpts::default();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].to_string_lossy().into_owned();
        let next = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            args.get(*i)
                .map(|s| s.to_string_lossy().into_owned())
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--menu" | "--dmenu" | "--stdin" => {}
            "--prompt" | "--title" => {
                let v = next(&mut i)?;
                if a == "--title" {
                    o.title = Some(v);
                } else {
                    o.prompt = Some(v);
                }
            }
            "--placeholder" => o.placeholder = Some(next(&mut i)?),
            "--layout" => o.layout = Some(next(&mut i)?),
            "--theme" => o.theme = Some(next(&mut i)?),
            "--multi" => o.multi = true,
            "--fuzzy" => o.fuzzy = true,
            "--case-sensitive" => o.case_sensitive = true,
            "--allow-custom" => o.allow_custom = true,
            "--history-file" => o.history_file = Some(next(&mut i)?),
            "--empty-message" => o.empty_message = Some(next(&mut i)?),
            "--max-results" => {
                o.max_results = Some(
                    next(&mut i)?
                        .parse()
                        .map_err(|_| "--max-results needs a number".to_string())?,
                )
            }
            "--min-match-chars" => {
                o.min_match_chars = Some(
                    next(&mut i)?
                        .parse()
                        .map_err(|_| "--min-match-chars needs a number".to_string())?,
                )
            }
            "--loading-message" => o.loading_message = Some(next(&mut i)?),
            "--footer-hint" => o.footer_hint = Some(next(&mut i)?),
            "--no-stream" => o.no_stream = true,
            "--format" => {
                o.format = match next(&mut i)?.as_str() {
                    "label" => OutputFormat::Label,
                    "id" => OutputFormat::Id,
                    "tag" => OutputFormat::Tag,
                    "json" => OutputFormat::Json,
                    other => return Err(format!("unknown output format {other:?}")),
                }
            }
            "--print-id" => o.format = OutputFormat::Id,
            "--shortcuts" => o.shortcuts = Some(PathBuf::from(next(&mut i)?)),
            "--no-shortcuts" => o.no_shortcuts = true,
            "--prefixes" => o.prefixes = Some(PathBuf::from(next(&mut i)?)),
            "--no-prefixes" => o.no_prefixes = true,
            "--config" => o.config = Some(PathBuf::from(next(&mut i)?)),
            "--app-id" => o.app_id = next(&mut i)?,
            "--pid-file" => o.pid_file = Some(PathBuf::from(next(&mut i)?)),
            "--password" => o.password = true,
            "--initial" => o.initial = Some(next(&mut i)?),
            "--message" => o.message = Some(next(&mut i)?),
            "--confirm-label" => o.confirm_label = Some(next(&mut i)?),
            "--cancel-label" => o.cancel_label = Some(next(&mut i)?),
            "--no-title" => o.show_title = Some(false),
            "--help" | "-h" => {
                return Err("use `r-modals --help` for usage".to_string());
            }
            s if s.starts_with('-') => return Err(format!("unknown flag {s}")),
            _ => {
                if o.positional.is_some() {
                    return Err(format!("unexpected argument {a:?}"));
                }
                o.positional = Some(a);
            }
        }
        i += 1;
    }
    Ok(o)
}

fn run_child_mode(mode: Mode, o: ChildOpts) -> i32 {
    let _pid = match o.pid_file.as_deref().map(write_pid_file) {
        Some(Ok(guard)) => Some(guard),
        Some(Err(e)) => {
            eprintln!("r-modals: cannot write pid file: {e}");
            return 2;
        }
        None => None,
    };
    match mode {
        Mode::Menu => run_menu(o),
        Mode::Ask => run_ask(o),
        Mode::Confirm => run_confirm(o),
        Mode::Error => run_error(o),
    }
}

fn run_menu(o: ChildOpts) -> i32 {
    let (theme, stylesheet) = match resolve_theme(o.theme.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("r-modals: {e}");
            return 2;
        }
    };
    let mut modal = Modal::new(o.prompt.clone().unwrap_or_else(|| "Pick one".to_string()))
        .theme(theme)
        .case_sensitive(o.case_sensitive)
        .fuzzy(o.fuzzy)
        .allow_custom_input(o.allow_custom)
        .app_id(o.app_id.clone());
    if let Some(css) = stylesheet {
        modal = modal.stylesheet(css);
    }
    if let Some(layout) = o.layout.as_deref() {
        modal = modal.layout(match layout {
            "grid" => crate::Layout::Grid,
            _ => crate::Layout::List,
        });
    }
    if let Some(p) = o.placeholder {
        modal = modal.placeholder(p);
    }
    let has_history = o.history_file.is_some();
    if let Some(h) = o.history_file {
        modal = modal.history_file(h);
    }
    if let Some(m) = o.empty_message {
        modal = modal.empty_message(m);
    }
    if let Some(m) = o.max_results {
        modal = modal.max_results(m);
    }
    if let Some(n) = o.min_match_chars {
        modal = modal.min_match_chars(n);
    }
    if let Some(h) = o.footer_hint {
        modal = modal.footer_hint(h);
    } else {
        modal = modal.footer_hint("Enter pick · Esc cancel");
    }
    if let Some(m) = o.loading_message {
        modal = modal.loading_message(m);
    }
    if o.multi {
        modal = modal.multi_select(true);
    }

    // `--config` supplies both sections; explicit per-section flags win.
    match (o.shortcuts, o.no_shortcuts) {
        (Some(p), _) => modal = modal.shortcuts_file(p),
        (None, true) => modal = modal.auto_shortcuts(false),
        (None, false) => {
            if let Some(p) = o.config.clone() {
                modal = modal.shortcuts_file(p);
            }
        }
    }
    match (o.prefixes, o.no_prefixes) {
        (Some(p), _) => modal = modal.prefixes_file(p),
        (None, true) => modal = modal.auto_prefixes(false),
        (None, false) => {
            if let Some(p) = o.config {
                modal = modal.prefixes_file(p);
            }
        }
    }

    if o.no_stream || o.multi || has_history {
        let options = match read_options_sync() {
            Ok(v) if !v.is_empty() => v,
            Ok(_) => {
                eprintln!("r-modals: no options on stdin");
                return 2;
            }
            Err(e) => {
                eprintln!("r-modals: cannot read stdin: {e}");
                return 2;
            }
        };
        let result = if o.multi {
            modal.options(options).run_multi().map(MenuResult::Many)
        } else {
            modal.options(options).run().map(MenuResult::One)
        };
        return finish_menu(result, o.format);
    }

    let (rx, saw, eof) = read_options_stream();
    let result = modal.run_streaming(rx).map(MenuResult::One);
    if let Err(e) = &result {
        eprintln!("r-modals: {e}");
        return 2;
    }
    match result.unwrap() {
        MenuResult::One(Some(sel)) => {
            print!("{}", format_selection(&sel, o.format));
            0
        }
        MenuResult::One(None) => {
            if !saw.load(Ordering::SeqCst) && eof.load(Ordering::SeqCst) {
                eprintln!("r-modals: no options on stdin");
                2
            } else {
                1
            }
        }
        MenuResult::Many(_) => unreachable!(),
    }
}

enum MenuResult {
    One(Option<ModalSelection>),
    Many(Vec<ModalSelection>),
}

fn finish_menu(result: Result<MenuResult, ModalError>, format: OutputFormat) -> i32 {
    match result {
        Ok(MenuResult::One(Some(sel))) => {
            print!("{}", format_selection(&sel, format));
            0
        }
        Ok(MenuResult::One(None)) => 1,
        Ok(MenuResult::Many(picks)) if picks.is_empty() => 1,
        Ok(MenuResult::Many(picks)) => {
            for sel in picks {
                print!("{}", format_selection(&sel, format));
            }
            0
        }
        Err(e) => {
            eprintln!("r-modals: {e}");
            2
        }
    }
}

fn read_options_sync() -> io::Result<Vec<ModalOption>> {
    let mut options = Vec::new();
    for line in io::stdin().lock().lines() {
        let line = line?;
        if let Some(opt) = parse_option_line(&line) {
            options.push(opt);
        }
    }
    Ok(options)
}

/// Open the window immediately; deliver the whole list when stdin reaches EOF.
fn read_options_stream() -> (Receiver<Vec<ModalOption>>, Arc<AtomicBool>, Arc<AtomicBool>) {
    let (tx, rx) = mpsc::channel();
    let saw = Arc::new(AtomicBool::new(false));
    let eof = Arc::new(AtomicBool::new(false));
    let saw_thread = saw.clone();
    let eof_thread = eof.clone();
    std::thread::spawn(move || {
        let mut options = Vec::new();
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if let Some(opt) = parse_option_line(&line) {
                options.push(opt);
            }
        }
        eof_thread.store(true, Ordering::SeqCst);
        if options.is_empty() {
            // Dropping the sender while still loading cancels the modal.
            return;
        }
        saw_thread.store(true, Ordering::SeqCst);
        let _ = tx.send(options);
    });
    (rx, saw, eof)
}

/// One stdin line: `label`, or `id<TAB>label`.
fn parse_option_line(line: &str) -> Option<ModalOption> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.is_empty() {
        return None;
    }
    match line.split_once('\t') {
        Some((id, label)) => {
            let id = if id.is_empty() { label } else { id };
            let label = if label.is_empty() { id } else { label };
            if id.is_empty() {
                None
            } else {
                Some(ModalOption::new(id, label))
            }
        }
        None => Some(ModalOption::new(line, line)),
    }
}

fn format_selection(sel: &ModalSelection, format: OutputFormat) -> String {
    match format {
        OutputFormat::Label => format!("{}\n", sel.label),
        OutputFormat::Id => format!("{}\n", sel.id),
        OutputFormat::Tag => format!(
            "{}\n{}\n",
            sel.shortcut.as_deref().unwrap_or_default(),
            sel.label
        ),
        OutputFormat::Json => format!(
            "{}\n",
            serde_json::to_string(sel).unwrap_or_else(|_| "{}".to_string())
        ),
    }
}

fn run_ask(o: ChildOpts) -> i32 {
    let (theme, stylesheet) = match resolve_theme(o.theme.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("r-modals: {e}");
            return 2;
        }
    };
    let label = o
        .positional
        .clone()
        .or_else(|| o.title.clone())
        .unwrap_or_else(|| "Input".to_string());
    let mut prompt = Prompt::new(label)
        .theme(theme)
        .app_id(o.app_id)
        .password(o.password)
        .show_title(o.show_title.unwrap_or(true));
    if let Some(css) = stylesheet {
        prompt = prompt.stylesheet(css);
    }
    if let Some(v) = o.initial {
        prompt = prompt.initial(v);
    }
    if let Some(v) = o.placeholder {
        prompt = prompt.placeholder(v);
    }
    if let Some(v) = o.message {
        prompt = prompt.message(v);
    }
    if let Some(v) = o.confirm_label {
        prompt = prompt.confirm_label(v);
    }
    match prompt.run() {
        Ok(Some(value)) => {
            println!("{value}");
            0
        }
        Ok(None) => 1,
        Err(e) => {
            eprintln!("r-modals: {e}");
            2
        }
    }
}

fn run_confirm(o: ChildOpts) -> i32 {
    let (theme, stylesheet) = match resolve_theme(o.theme.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("r-modals: {e}");
            return 2;
        }
    };
    let title = o
        .positional
        .clone()
        .or_else(|| o.title.clone())
        .unwrap_or_else(|| "Confirm".to_string());
    let mut confirm = Confirm::new(title)
        .theme(theme)
        .app_id(o.app_id)
        .show_title(o.show_title.unwrap_or(true));
    if let Some(css) = stylesheet {
        confirm = confirm.stylesheet(css);
    }
    if let Some(v) = o.message {
        confirm = confirm.message(v);
    }
    if let Some(v) = o.confirm_label {
        confirm = confirm.confirm_label(v);
    }
    if let Some(v) = o.cancel_label {
        confirm = confirm.cancel_label(v);
    }
    match confirm.run() {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(e) => {
            eprintln!("r-modals: {e}");
            2
        }
    }
}

fn run_error(o: ChildOpts) -> i32 {
    let (theme, stylesheet) = match resolve_theme(o.theme.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("r-modals: {e}");
            return 2;
        }
    };
    let title = o.title.unwrap_or_else(|| "r-modals".to_string());
    let message = o.positional.or(o.message).unwrap_or_default();
    let mut confirm = Confirm::new(title)
        .theme(theme)
        .app_id(o.app_id)
        .show_title(o.show_title.unwrap_or(true))
        .message(message)
        .confirm_label(o.confirm_label.unwrap_or_else(|| "OK".to_string()))
        .cancel_label(o.cancel_label.unwrap_or_else(|| "Close".to_string()));
    if let Some(css) = stylesheet {
        confirm = confirm.stylesheet(css);
    }
    match confirm.run() {
        // Both buttons dismiss an info dialog.
        Ok(_) => 0,
        Err(e) => {
            eprintln!("r-modals: {e}");
            2
        }
    }
}

/// Resolve `--theme`: a preset name, a CSS file path, or inline CSS.
pub(crate) fn resolve_theme(spec: Option<&str>) -> Result<(Theme, Option<String>), String> {
    let Some(spec) = spec else {
        return Ok((Theme::dark(), None));
    };
    match spec {
        "dark" => Ok((Theme::dark(), None)),
        "light" => Ok((Theme::light(), None)),
        "transparent" | "transparent_dark" | "transparent-dark" => {
            Ok((Theme::transparent_dark(), None))
        }
        "auto" => Ok((Theme::auto(), None)),
        other => {
            if Path::new(other).is_file() {
                let css = std::fs::read_to_string(other)
                    .map_err(|e| format!("cannot read theme file {other:?}: {e}"))?;
                return Ok((Theme::dark(), Some(css)));
            }
            Ok((Theme::dark(), Some(other.to_string())))
        }
    }
}

/// Write the child pid atomically and remove it when the child exits.
fn write_pid_file(path: &Path) -> io::Result<PidGuard> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, format!("{}\n", std::process::id()))?;
    std::fs::rename(&tmp, path)?;
    Ok(PidGuard {
        path: path.to_path_buf(),
    })
}

struct PidGuard {
    path: PathBuf,
}

impl Drop for PidGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Exit the process when the first argument is a dialog mode.
///
/// ```no_run
/// r_modals::dialog_main!();
/// // ...
/// ```
#[macro_export]
macro_rules! dialog_main {
    () => {
        if let Some(code) = $crate::run_child_from_args() {
            std::process::exit(code);
        }
    };
}

#[cfg(test)]
mod tests;
