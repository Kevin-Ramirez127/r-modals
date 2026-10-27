//! Put a newly created window on a compositor workspace.
//!
//! `egui`/`eframe` has no portable way to choose a workspace, so this module
//! asks the running compositor to move the window after it is mapped. It
//! supports Hyprland, Sway, i3, niri, KDE Plasma (via `kdotool`), and generic
//! X11 desktops (via `wmctrl`). Placement is best-effort: an unsupported
//! compositor or a missing helper leaves the window on the current workspace.

use serde_json::Value;
use std::process::Command;
use std::str::FromStr;
use std::time::{Duration, Instant};

/// How long to wait for a new window to show up in the compositor's window
/// list before giving up.
const PLACEMENT_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Compositor used to move a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compositor {
    /// Detect the compositor from the environment (default).
    #[default]
    Auto,
    /// Hyprland (`hyprctl`).
    Hyprland,
    /// Sway (`swaymsg`).
    Sway,
    /// i3 (`i3-msg`).
    I3,
    /// niri (`niri msg`).
    Niri,
    /// KDE Plasma (`kdotool`, with an X11 `wmctrl` fallback).
    Kde,
    /// A generic X11 desktop (`wmctrl`).
    X11,
    /// GNOME Shell. X11 sessions fall back to `wmctrl`; GNOME Wayland has no
    /// application-level workspace API.
    Gnome,
    /// No compositor was detected.
    Unknown,
}

impl Compositor {
    /// Detect the compositor from environment variables.
    pub fn detect() -> Self {
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
            return Self::Hyprland;
        }
        if std::env::var_os("SWAYSOCK").is_some() {
            return Self::Sway;
        }
        if std::env::var_os("I3SOCK").is_some() {
            return Self::I3;
        }
        if std::env::var_os("NIRI_SOCKET").is_some() {
            return Self::Niri;
        }

        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| std::env::var("XDG_SESSION_DESKTOP"))
            .or_else(|_| std::env::var("DESKTOP_SESSION"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if desktop.contains("hyprland") {
            return Self::Hyprland;
        }
        if desktop.contains("sway") {
            return Self::Sway;
        }
        if desktop.contains("niri") {
            return Self::Niri;
        }
        if desktop.contains("kde") || desktop.contains("plasma") {
            return Self::Kde;
        }
        if desktop.contains("gnome") {
            return Self::Gnome;
        }
        if desktop.contains("i3") {
            return Self::I3;
        }

        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            Self::Unknown
        } else if std::env::var_os("DISPLAY").is_some() {
            Self::X11
        } else {
            Self::Unknown
        }
    }
}

impl std::fmt::Display for Compositor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Auto => "auto",
            Self::Hyprland => "hyprland",
            Self::Sway => "sway",
            Self::I3 => "i3",
            Self::Niri => "niri",
            Self::Kde => "kde",
            Self::X11 => "x11",
            Self::Gnome => "gnome",
            Self::Unknown => "unknown",
        })
    }
}

impl FromStr for Compositor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "hyprland" | "hypr" => Ok(Self::Hyprland),
            "sway" => Ok(Self::Sway),
            "i3" => Ok(Self::I3),
            "niri" => Ok(Self::Niri),
            "kde" | "plasma" => Ok(Self::Kde),
            "x11" | "xorg" => Ok(Self::X11),
            "gnome" => Ok(Self::Gnome),
            "unknown" | "none" => Ok(Self::Unknown),
            other => Err(format!("unknown compositor {other:?}")),
        }
    }
}

/// Where a newly created window should be placed.
///
/// The name is passed to the compositor as-is, so `"2"` is workspace number
/// two on Hyprland/Sway/niri while `"code"` is a named workspace. Use
/// [`Workspace::special`] for a Hyprland special (scratchpad) workspace.
///
/// ```no_run
/// use r_modals::{Modal, Workspace};
///
/// Modal::new("Pick one")
///     .workspace(Workspace::new("2"))
///     .run()
///     .unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    name: String,
    focus: bool,
    compositor: Option<Compositor>,
}

impl Workspace {
    /// A workspace by number or name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            focus: true,
            compositor: None,
        }
    }

    /// A workspace by numeric index (`2` becomes `"2"`).
    pub fn index(index: u32) -> Self {
        Self::new(index.to_string())
    }

    /// A Hyprland special (scratchpad) workspace: `special:<name>`.
    pub fn special(name: impl Into<String>) -> Self {
        Self::new(format!("special:{}", name.into()))
    }

    /// Switch to the workspace after moving the window. Defaults to `true`.
    ///
    /// With `false`, the compositor is asked to move the window without
    /// explicitly focusing the target workspace. Some compositors still
    /// switch when the moved window has focus.
    pub fn focus(mut self, yes: bool) -> Self {
        self.focus = yes;
        self
    }

    /// Override compositor detection.
    pub fn compositor(mut self, compositor: Compositor) -> Self {
        self.compositor = Some(compositor);
        self
    }

    /// Target workspace name or number.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether the target workspace should be focused after the move.
    pub fn focused(&self) -> bool {
        self.focus
    }

    /// Explicit compositor override, if any.
    pub fn compositor_override(&self) -> Option<Compositor> {
        self.compositor
    }

    /// Move an already-created window to this workspace.
    ///
    /// `pid`, `app_id`, and `title` identify the window. The compositor's
    /// window list is polled for up to three seconds because the window may
    /// not be mapped yet.
    pub fn move_window(&self, pid: u32, app_id: &str, title: &str) -> Result<(), WorkspaceError> {
        place_when_ready(self, pid, app_id, title)
    }
}

impl From<&str> for Workspace {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl From<String> for Workspace {
    fn from(name: String) -> Self {
        Self::new(name)
    }
}

impl From<&String> for Workspace {
    fn from(name: &String) -> Self {
        Self::new(name.clone())
    }
}

impl From<u32> for Workspace {
    fn from(index: u32) -> Self {
        Self::index(index)
    }
}

/// Why a workspace move failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    /// The compositor has no supported IPC, or a helper binary is missing.
    Unsupported(String),
    /// The window did not appear in the compositor's window list in time.
    NotFound,
    /// A compositor/helper command failed.
    Command(String),
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(e) => write!(f, "workspace placement unsupported: {e}"),
            Self::NotFound => write!(f, "workspace placement: window not found"),
            Self::Command(e) => write!(f, "workspace placement command failed: {e}"),
        }
    }
}

impl std::error::Error for WorkspaceError {}

/// Spawn a detached best-effort placement attempt for a new window.
pub(crate) fn spawn(workspace: Workspace, pid: u32, app_id: String, title: String) {
    if workspace.name.trim().is_empty() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("r-modals-workspace".to_string())
        .spawn(move || {
            let _ = place_when_ready(&workspace, pid, &app_id, &title);
        });
}

fn place_when_ready(
    workspace: &Workspace,
    pid: u32,
    app_id: &str,
    title: &str,
) -> Result<(), WorkspaceError> {
    let compositor = workspace.compositor.unwrap_or_else(Compositor::detect);
    if compositor == Compositor::Auto {
        return Err(WorkspaceError::Unsupported(
            "compositor could not be detected".to_string(),
        ));
    }

    let start = Instant::now();
    loop {
        match try_place(compositor, workspace, pid, app_id, title) {
            Ok(()) => return Ok(()),
            Err(WorkspaceError::NotFound) if start.elapsed() < PLACEMENT_TIMEOUT => {
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => return Err(e),
        }
    }
}

fn try_place(
    compositor: Compositor,
    workspace: &Workspace,
    pid: u32,
    app_id: &str,
    title: &str,
) -> Result<(), WorkspaceError> {
    let name = workspace.name.as_str();
    match compositor {
        Compositor::Hyprland => place_hyprland(workspace, pid, app_id, title),
        Compositor::Sway => place_sway("swaymsg", workspace, pid, app_id, title),
        Compositor::I3 => place_sway("i3-msg", workspace, pid, app_id, title),
        Compositor::Niri => place_niri(workspace, pid, app_id, title),
        Compositor::Kde => place_kde(workspace, pid, app_id, title),
        Compositor::X11 => place_x11(workspace, pid, app_id, title),
        // GNOME on X11 still has EWMH desktops; GNOME Wayland does not expose
        // a workspace-move API to applications.
        Compositor::Gnome
            if std::env::var_os("DISPLAY").is_some()
                && std::env::var_os("WAYLAND_DISPLAY").is_none() =>
        {
            place_x11(workspace, pid, app_id, title)
        }
        Compositor::Gnome => Err(WorkspaceError::Unsupported(
            "GNOME Shell does not expose a workspace-move API to applications".to_string(),
        )),
        Compositor::Unknown | Compositor::Auto => Err(WorkspaceError::Unsupported(format!(
            "no supported compositor detected for workspace {name:?}"
        ))),
    }
}

fn run(program: &str, args: &[&str]) -> Result<std::process::Output, WorkspaceError> {
    let output = Command::new(program).args(args).output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            WorkspaceError::Unsupported(format!("{program} is not installed"))
        } else {
            WorkspaceError::Command(format!("cannot run {program}: {e}"))
        }
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        return Err(WorkspaceError::Command(if stderr.is_empty() {
            format!("{program} exited with {}", output.status)
        } else {
            format!("{program}: {stderr}")
        }));
    }
    Ok(output)
}

// ---------------------------------------------------------------------------
// Hyprland

fn place_hyprland(
    workspace: &Workspace,
    pid: u32,
    app_id: &str,
    title: &str,
) -> Result<(), WorkspaceError> {
    let output = run("hyprctl", &["clients", "-j"])?;
    let address = parse_hyprland_clients(&output.stdout, pid, app_id, title)
        .ok_or(WorkspaceError::NotFound)?;
    let dispatch = if workspace.focus {
        "movetoworkspace"
    } else {
        "movetoworkspacesilent"
    };
    let target = format!("{},address:{}", workspace.name, address);
    run("hyprctl", &["dispatch", dispatch, &target])?;
    Ok(())
}

fn parse_hyprland_clients(json: &[u8], pid: u32, app_id: &str, title: &str) -> Option<String> {
    let clients: Vec<Value> = serde_json::from_slice(json).ok()?;
    let mut best: Option<(u8, String)> = None;
    for client in clients {
        if client.get("pid").and_then(Value::as_u64) != Some(u64::from(pid)) {
            continue;
        }
        let Some(address) = client.get("address").and_then(Value::as_str) else {
            continue;
        };
        let class = client.get("class").and_then(Value::as_str).unwrap_or("");
        let initial = client
            .get("initialClass")
            .and_then(Value::as_str)
            .unwrap_or("");
        let window_title = client.get("title").and_then(Value::as_str).unwrap_or("");

        let mut score = 1;
        if class == app_id || initial == app_id {
            score += 2;
        }
        if window_title == title {
            score += 2;
        }
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            best = Some((score, address.to_string()));
        }
    }
    best.map(|(_, address)| address)
}

// ---------------------------------------------------------------------------
// Sway / i3

fn place_sway(
    program: &str,
    workspace: &Workspace,
    pid: u32,
    app_id: &str,
    title: &str,
) -> Result<(), WorkspaceError> {
    let output = run(program, &["-t", "get_tree", "-r"])?;
    let tree: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| WorkspaceError::Command(e.to_string()))?;
    let con_id = find_sway_container(&tree, pid, app_id, title).ok_or(WorkspaceError::NotFound)?;
    let ws = sway_token(&workspace.name);
    let move_cmd = format!("[con_id={con_id}] move container to workspace {ws}");
    run(program, &[&move_cmd])?;
    if workspace.focus {
        let focus_cmd = format!("workspace {ws}");
        run(program, &[&focus_cmd])?;
    }
    Ok(())
}

fn find_sway_container(node: &Value, pid: u32, app_id: &str, title: &str) -> Option<u64> {
    let mut best: Option<(u8, u64)> = None;
    walk_sway_tree(node, pid, app_id, title, &mut best);
    best.map(|(_, id)| id)
}

fn walk_sway_tree(node: &Value, pid: u32, app_id: &str, title: &str, best: &mut Option<(u8, u64)>) {
    if node.get("pid").and_then(Value::as_u64) == Some(u64::from(pid)) {
        if let Some(id) = node.get("id").and_then(Value::as_u64) {
            let wayland_app = node.get("app_id").and_then(Value::as_str).unwrap_or("");
            let class = node
                .get("window_properties")
                .and_then(|p| p.get("class"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let name = node.get("name").and_then(Value::as_str).unwrap_or("");

            let mut score = 1;
            if wayland_app == app_id || class == app_id {
                score += 2;
            }
            if name == title {
                score += 2;
            }
            if best
                .as_ref()
                .is_none_or(|(best_score, _)| score > *best_score)
            {
                *best = Some((score, id));
            }
        }
    }

    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node.get(key).and_then(Value::as_array) {
            for child in children {
                walk_sway_tree(child, pid, app_id, title, best);
            }
        }
    }
}

fn sway_token(name: &str) -> String {
    if name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
    {
        name.to_string()
    } else {
        format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

// ---------------------------------------------------------------------------
// niri

fn place_niri(
    workspace: &Workspace,
    pid: u32,
    app_id: &str,
    title: &str,
) -> Result<(), WorkspaceError> {
    let output = run("niri", &["msg", "--json", "windows"])?;
    let id =
        parse_niri_windows(&output.stdout, pid, app_id, title).ok_or(WorkspaceError::NotFound)?;
    let id = id.to_string();
    let ws = workspace.name.as_str();
    let mut args = vec![
        "msg",
        "action",
        "move-window-to-workspace",
        "--window-id",
        id.as_str(),
    ];
    if !workspace.focus {
        args.extend(["--focus", "false"]);
    }
    args.push(ws);
    run("niri", &args)?;
    if workspace.focus {
        run("niri", &["msg", "action", "focus-workspace", ws])?;
    }
    Ok(())
}

fn parse_niri_windows(json: &[u8], pid: u32, app_id: &str, title: &str) -> Option<u64> {
    let value: Value = serde_json::from_slice(json).ok()?;
    let windows = match &value {
        Value::Array(windows) => windows.as_slice(),
        Value::Object(map) => map
            .get("Ok")
            .and_then(|ok| match ok {
                Value::Array(windows) => Some(windows.as_slice()),
                Value::Object(inner) => inner
                    .get("Windows")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice),
                _ => None,
            })
            .unwrap_or(&[]),
        _ => &[],
    };

    let mut best: Option<(u8, u64)> = None;
    for window in windows {
        if window.get("pid").and_then(Value::as_u64) != Some(u64::from(pid)) {
            continue;
        }
        let Some(id) = window.get("id").and_then(Value::as_u64) else {
            continue;
        };
        let window_app = window.get("app_id").and_then(Value::as_str).unwrap_or("");
        let window_title = window.get("title").and_then(Value::as_str).unwrap_or("");
        let mut score = 1;
        if window_app == app_id {
            score += 2;
        }
        if window_title == title {
            score += 2;
        }
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            best = Some((score, id));
        }
    }
    best.map(|(_, id)| id)
}

// ---------------------------------------------------------------------------
// KDE Plasma

fn place_kde(
    workspace: &Workspace,
    pid: u32,
    app_id: &str,
    title: &str,
) -> Result<(), WorkspaceError> {
    let desktop = workspace.name.trim();
    if !desktop.chars().all(|c| c.is_ascii_digit()) {
        return Err(WorkspaceError::Unsupported(
            "kdotool only supports numeric KDE desktops".to_string(),
        ));
    }

    let search = match run("kdotool", &["search", "--pid", &pid.to_string()]) {
        Ok(output) => output,
        Err(WorkspaceError::Unsupported(_)) if std::env::var_os("DISPLAY").is_some() => {
            return place_x11(workspace, pid, app_id, title);
        }
        Err(e) => return Err(e),
    };
    let stdout = String::from_utf8_lossy(&search.stdout);
    let ids: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let id = ids.first().ok_or(WorkspaceError::NotFound)?;
    run("kdotool", &["set_desktop_for_window", id, desktop])?;
    if workspace.focus {
        run("kdotool", &["set_desktop", desktop])?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Generic X11

fn place_x11(
    workspace: &Workspace,
    pid: u32,
    _app_id: &str,
    title: &str,
) -> Result<(), WorkspaceError> {
    let desktop = x11_desktop_index(&workspace.name)?;
    let output = run("wmctrl", &["-lp"])?;
    let window = parse_wmctrl_lp(&output.stdout, pid, title).ok_or(WorkspaceError::NotFound)?;
    run("wmctrl", &["-i", "-r", &window, "-t", &desktop])?;
    if workspace.focus {
        run("wmctrl", &["-s", &desktop])?;
    }
    Ok(())
}

fn x11_desktop_index(name: &str) -> Result<String, WorkspaceError> {
    let name = name.trim();
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) {
        return Ok(name.to_string());
    }
    let output = run("wmctrl", &["-d"])?;
    let desktops = parse_wmctrl_desktops(&String::from_utf8_lossy(&output.stdout));
    desktops
        .iter()
        .find(|(_, desktop_name)| desktop_name == name)
        .map(|(index, _)| index.to_string())
        .ok_or_else(|| WorkspaceError::Unsupported(format!("X11 desktop {name:?} does not exist")))
}

fn parse_wmctrl_lp(output: &[u8], pid: u32, title: &str) -> Option<String> {
    let text = String::from_utf8_lossy(output);
    let mut best: Option<(u8, String)> = None;
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(id) = fields.first() else { continue };
        let Some(window_pid) = fields.get(2).and_then(|p| p.parse::<u32>().ok()) else {
            continue;
        };
        if window_pid != pid {
            continue;
        }
        let window_title = fields.get(4..).unwrap_or(&[]).join(" ");
        let score = if window_title == title { 2 } else { 1 };
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            best = Some((score, (*id).to_string()));
        }
    }
    best.map(|(_, id)| id)
}

fn parse_wmctrl_desktops(text: &str) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some(index) = fields.first().and_then(|f| f.parse::<u32>().ok()) else {
            continue;
        };
        let Some(wa) = fields.iter().position(|f| *f == "WA:") else {
            continue;
        };
        let name = fields.get(wa + 3..).unwrap_or(&[]).join(" ");
        out.push((index, name));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_constructors_and_options() {
        assert_eq!(Workspace::new("2").name(), "2");
        assert_eq!(Workspace::index(3).name(), "3");
        assert_eq!(Workspace::special("scratch").name(), "special:scratch");
        assert!(Workspace::new("2").focused());
        assert!(!Workspace::new("2").focus(false).focused());
        assert_eq!(
            Workspace::new("2")
                .compositor(Compositor::Hyprland)
                .compositor_override(),
            Some(Compositor::Hyprland)
        );
        assert_eq!(Workspace::from("code").name(), "code");
        assert_eq!(Workspace::from(4u32).name(), "4");
    }

    #[test]
    fn compositor_names_round_trip() {
        for (name, compositor) in [
            ("auto", Compositor::Auto),
            ("hyprland", Compositor::Hyprland),
            ("sway", Compositor::Sway),
            ("i3", Compositor::I3),
            ("niri", Compositor::Niri),
            ("kde", Compositor::Kde),
            ("x11", Compositor::X11),
            ("gnome", Compositor::Gnome),
            ("unknown", Compositor::Unknown),
        ] {
            assert_eq!(Compositor::from_str(name), Ok(compositor));
            assert_eq!(compositor.to_string(), name);
        }
        assert!(Compositor::from_str("awesome").is_err());
    }

    #[test]
    fn hyprland_clients_match_by_app_id_then_title() {
        let json = br#"[
            {"pid": 7, "address": "0xaaa", "class": "other", "initialClass": "other", "title": "Other"},
            {"pid": 7, "address": "0xbbb", "class": "r-modals", "initialClass": "r-modals", "title": "Picker"},
            {"pid": 8, "address": "0xccc", "class": "r-modals", "initialClass": "r-modals", "title": "Picker"}
        ]"#;
        assert_eq!(
            parse_hyprland_clients(json, 7, "r-modals", "Picker").as_deref(),
            Some("0xbbb")
        );
        assert_eq!(
            parse_hyprland_clients(json, 7, "missing", "Other").as_deref(),
            Some("0xaaa")
        );
        assert_eq!(parse_hyprland_clients(json, 9, "r-modals", "Picker"), None);
    }

    #[test]
    fn sway_tree_matches_nested_windows() {
        let tree: Value = serde_json::from_str(
            r#"{
                "id": 1, "pid": null, "nodes": [
                    {"id": 2, "pid": 7, "app_id": "other", "name": "Other", "nodes": [], "floating_nodes": []},
                    {"id": 3, "pid": 7, "app_id": "r-modals", "name": "Picker", "nodes": [], "floating_nodes": []}
                ], "floating_nodes": []
            }"#,
        )
        .unwrap();
        assert_eq!(find_sway_container(&tree, 7, "r-modals", "Picker"), Some(3));
        assert_eq!(find_sway_container(&tree, 7, "missing", "Other"), Some(2));
        assert_eq!(find_sway_container(&tree, 9, "r-modals", "Picker"), None);
    }

    #[test]
    fn sway_workspace_names_are_quoted_when_needed() {
        assert_eq!(sway_token("2"), "2");
        assert_eq!(sway_token("code"), "code");
        assert_eq!(sway_token("my ws"), "\"my ws\"");
        assert_eq!(sway_token("a\"b"), "\"a\\\"b\"");
    }

    #[test]
    fn niri_windows_parse_arrays_and_wrapped_objects() {
        let array = br#"[
            {"id": 5, "pid": 7, "app_id": "other", "title": "Other"},
            {"id": 6, "pid": 7, "app_id": "r-modals", "title": "Picker"}
        ]"#;
        assert_eq!(parse_niri_windows(array, 7, "r-modals", "Picker"), Some(6));
        assert_eq!(parse_niri_windows(array, 7, "missing", "Other"), Some(5));

        let wrapped = br#"{"Ok": [{"id": 9, "pid": 7, "app_id": "r-modals", "title": "Picker"}]}"#;
        assert_eq!(
            parse_niri_windows(wrapped, 7, "r-modals", "Picker"),
            Some(9)
        );
    }

    #[test]
    fn wmctrl_window_and_desktop_parsing() {
        let windows = b"0x03a00007  0 1234 host Picker\n0x03a00008  1 1234 host Other\n";
        assert_eq!(
            parse_wmctrl_lp(windows, 1234, "Other").as_deref(),
            Some("0x03a00008")
        );
        assert_eq!(parse_wmctrl_lp(windows, 99, "Other"), None);

        let desktops = "0  * DG: 1920x1080  VP: 0,0  WA: 0,0 1920x1080  main\n\
                        1  - DG: 1920x1080  VP: 0,0  WA: 0,0 1920x1080  side work\n";
        assert_eq!(
            parse_wmctrl_desktops(desktops),
            vec![(0, "main".to_string()), (1, "side work".to_string())]
        );
    }
}
