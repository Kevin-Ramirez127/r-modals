use std::path::{Path, PathBuf};

/// Image attached to an option.
///
/// Loaded lazily at render time via the `image` crate; failures
/// fall back to no icon instead of crashing the modal.
#[derive(Debug, Clone, Default)]
pub enum IconSource {
    /// No image.
    #[default]
    None,
    /// Load from a file path (png/jpg/svg-bitmap/etc. supported by `image`).
    File(PathBuf),
    /// Raw encoded bytes (e.g. `include_bytes!("icon.png")`).
    Bytes(Vec<u8>),
    /// Raw RGBA pixels.
    Pixels {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
    },
}

impl IconSource {
    /// Stable cache key for the egui texture cache.
    ///
    /// NOTE: this can hash full image bytes — call it once per option and
    /// reuse the result (see `ModalApp::icon_keys`). Calling it per frame
    /// would re-hash megabytes of pixel data at 60fps.
    pub(crate) fn cache_key(&self, option_id: &str) -> Option<String> {
        match self {
            IconSource::None => None,
            IconSource::File(p) => Some(format!("icon:{option_id}:file:{}", p.display())),
            IconSource::Bytes(b) => Some(format!("icon:{option_id}:bytes:{}:{:x}", b.len(), {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                b.hash(&mut h);
                h.finish()
            })),
            IconSource::Pixels {
                width,
                height,
                rgba,
            } => {
                // Include a content hash: same dims + same id but different
                // pixels must not share a stale texture.
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                rgba.hash(&mut h);
                Some(format!(
                    "icon:{option_id}:px:{width}x{height}:{:x}",
                    h.finish()
                ))
            }
        }
    }
}

impl From<&str> for IconSource {
    fn from(s: &str) -> Self {
        IconSource::File(PathBuf::from(s))
    }
}

impl From<&Path> for IconSource {
    fn from(p: &Path) -> Self {
        IconSource::File(p.to_path_buf())
    }
}

impl From<PathBuf> for IconSource {
    fn from(p: PathBuf) -> Self {
        IconSource::File(p)
    }
}

impl From<Vec<u8>> for IconSource {
    fn from(b: Vec<u8>) -> Self {
        IconSource::Bytes(b)
    }
}

impl From<&[u8]> for IconSource {
    fn from(b: &[u8]) -> Self {
        IconSource::Bytes(b.to_vec())
    }
}

/// Fallback fired when an option's [`SelectAction`] fails to spawn.
/// Runs on the calling thread, right where the action would have run.
pub type ErrorAction =
    std::sync::Arc<dyn Fn(&crate::ModalSelection, &std::io::Error) + Send + Sync>;
/// What happens when an option is picked (see [`ModalOption::command`],
/// [`ModalOption::shell`], [`ModalOption::on_select`]).
///
/// Executed once by [`Modal::run`] after the window closes, on the calling
/// thread. Process launches are best-effort and spawn detached: a failing
/// spawn never erases the pick.
#[derive(Clone)]
pub enum SelectAction {
    /// Spawn `program` with `args` detached (`echo`, `firefox`, …).
    Command { program: String, args: Vec<String> },
    /// Run a shell one-liner (`sh -c` on unix, `cmd /C` on Windows).
    Shell(String),
    /// Call a Rust closure with the final selection.
    Callback(std::sync::Arc<dyn Fn(&crate::ModalSelection) + Send + Sync>),
}

impl std::fmt::Debug for SelectAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectAction::Command { program, args } => f
                .debug_struct("Command")
                .field("program", program)
                .field("args", args)
                .finish(),
            SelectAction::Shell(cmd) => f.debug_tuple("Shell").field(cmd).finish(),
            SelectAction::Callback(_) => f.debug_tuple("Callback").field(&"..").finish(),
        }
    }
}

impl SelectAction {
    /// Fire the action for a pick (called by [`Modal::run`](crate::Modal::run)
    /// on the calling thread, after the window closes). `Ok` for callbacks;
    /// the spawn result for processes.
    pub(crate) fn execute(&self, sel: &crate::ModalSelection) -> std::io::Result<()> {
        match self {
            SelectAction::Command { program, args } => std::process::Command::new(program)
                .args(args)
                .spawn()
                .map(|_| ()),
            SelectAction::Shell(cmd) => {
                let (prog, flag) = if cfg!(windows) {
                    ("cmd", "/C")
                } else {
                    ("sh", "-c")
                };
                std::process::Command::new(prog)
                    .args([flag, cmd])
                    .spawn()
                    .map(|_| ())
            }
            SelectAction::Callback(f) => {
                f(sel);
                Ok(())
            }
        }
    }
}

/// One selectable row in the modal.
///
/// An option with [`ModalOption::sub_option`] children acts as a submenu:
/// activating it drills one level deeper instead of picking it. Only
/// childless (leaf) options can be picked.
#[derive(Clone)]
pub struct ModalOption {
    /// Stable id returned on selection.
    pub id: String,
    /// Primary text (what search matches against).
    pub label: String,
    /// Secondary dimmed text.
    pub description: Option<String>,
    /// Optional image shown left of the label.
    pub icon: IconSource,
    /// Hides the row from search but keeps it selectable via arrow keys.
    /// Useful for separators/headers. Defaults to false.
    /// (A hidden parent still surfaces as a gateway when one of its
    /// descendants matches the query.)
    pub unsearchable: bool,
    /// Nested options shown one level deeper on drill-down.
    /// Empty (default) means this option is a pickable leaf.
    pub children: Vec<ModalOption>,
    /// Action fired when this option is picked (see [`SelectAction`]).
    /// `None` (default) means pick-only. Process launches are best-effort:
    /// a failing spawn fires [`ModalOption::on_error`] instead of erasing
    /// the pick.
    pub action: Option<SelectAction>,
    /// Fallback fired when this option's [`SelectAction`] fails to spawn.
    /// `None` (default) means failures stay silent. Never fires without an
    /// action, and never fires for Rust callbacks (they cannot spawn-fail).
    pub on_error: Option<ErrorAction>,
    /// When true the row is shown dimmed and cannot be picked or drilled
    /// into. Defaults to false.
    pub disabled: bool,
    /// When true the row is a visual separator/header: never pickable,
    /// never matches a non-empty query, always listed on empty query.
    /// Defaults to false.
    pub separator: bool,
    /// Optional keyboard-shortcut hint rendered right-aligned in list rows
    /// (e.g. `"Ctrl+O"`). Purely visual. Defaults to `None`.
    pub shortcut: Option<String>,
    /// Extra search keywords matched alongside label/id/description
    /// (both substring and fuzzy modes). Defaults to empty.
    pub keywords: Vec<String>,
}

impl std::fmt::Debug for ModalOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModalOption")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("description", &self.description)
            .field("icon", &self.icon)
            .field("unsearchable", &self.unsearchable)
            .field("children", &self.children)
            .field("action", &self.action)
            .field("on_error", &self.on_error.as_ref().map(|_| ".."))
            .field("disabled", &self.disabled)
            .field("separator", &self.separator)
            .field("shortcut", &self.shortcut)
            .field("keywords", &self.keywords)
            .finish()
    }
}

impl ModalOption {
    /// Create an option. `id` is returned in [`crate::ModalSelection`].
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            icon: IconSource::None,
            unsearchable: false,
            children: Vec::new(),
            action: None,
            on_error: None,
            disabled: false,
            separator: false,
            shortcut: None,
            keywords: Vec::new(),
        }
    }

    /// Visual separator/header row. Never pickable and never matches a
    /// non-empty query; listed on empty queries so sections keep their shape.
    pub fn separator(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            separator: true,
            ..Self::new(id, label)
        }
    }

    /// Secondary text shown under / beside the label.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Attach an image. Accepts `IconSource`, `&str`/`PathBuf` (file), or `Vec<u8>` (bytes).
    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.icon = icon.into();
        self
    }

    /// Raw RGBA icon.
    pub fn icon_pixels(mut self, width: usize, height: usize, rgba: Vec<u8>) -> Self {
        self.icon = IconSource::Pixels {
            width,
            height,
            rgba,
        };
        self
    }

    /// Exclude this row from text filtering.
    pub fn unsearchable(mut self, yes: bool) -> Self {
        self.unsearchable = yes;
        self
    }

    /// Mark this option as disabled: shown dimmed, skipped by activation
    /// (Enter/click does nothing). Still listed and searchable.
    pub fn disabled(mut self, yes: bool) -> Self {
        self.disabled = yes;
        self
    }

    /// Shortcut hint rendered right-aligned in list rows (visual only).
    pub fn shortcut(mut self, hint: impl Into<String>) -> Self {
        self.shortcut = Some(hint.into());
        self
    }

    /// Extra search keywords (matched in both substring and fuzzy modes).
    pub fn keywords<I, S>(mut self, kws: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.keywords.extend(kws.into_iter().map(Into::into));
        self
    }

    /// Single extra search keyword.
    pub fn keyword(mut self, kw: impl Into<String>) -> Self {
        self.keywords.push(kw.into());
        self
    }

    /// Whether this row can ever be activated (picked or drilled into).
    /// Separators and disabled rows are never activatable.
    pub fn activatable(&self) -> bool {
        !self.separator && !self.disabled
    }

    /// Nest one sub-option under this option, turning it into a submenu.
    /// Activating a parent drills one level deeper; only leaves can be
    /// picked. Nests arbitrarily deep.
    pub fn sub_option(mut self, opt: ModalOption) -> Self {
        self.children.push(opt);
        self
    }

    /// Nest many sub-options under this option (see [`ModalOption::sub_option`]).
    pub fn sub_options(mut self, opts: impl IntoIterator<Item = ModalOption>) -> Self {
        self.children.extend(opts);
        self
    }

    /// True when this option has children (a submenu: activates to drill
    /// in rather than picking).
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Run `program` when this option is picked, spawned detached.
    /// Combine with [`ModalOption::arg`]/[`ModalOption::args`] for flags:
    ///
    /// ```no_run
    /// use r_modals::ModalOption;
    /// let opt = ModalOption::new("ff", "Firefox").command("firefox").arg("--new-window");
    /// ```
    pub fn command(mut self, program: impl Into<String>) -> Self {
        self.action = Some(SelectAction::Command {
            program: program.into(),
            args: Vec::new(),
        });
        self
    }

    /// Append one argument to this option's [`ModalOption::command`].
    /// No-op unless a command was set (then kept in order).
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        if let Some(SelectAction::Command { args, .. }) = &mut self.action {
            args.push(arg.into());
        }
        self
    }

    /// Append many arguments to this option's [`ModalOption::command`].
    /// No-op unless a command was set.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if let Some(SelectAction::Command { args: dst, .. }) = &mut self.action {
            dst.extend(args.into_iter().map(Into::into));
        }
        self
    }

    /// Run a shell one-liner when this option is picked
    /// (`sh -c` on unix, `cmd /C` on Windows), spawned detached:
    ///
    /// ```no_run
    /// use r_modals::ModalOption;
    /// let opt = ModalOption::new("up", "Update").shell("notify-send done && apt update");
    /// ```
    pub fn shell(mut self, cmdline: impl Into<String>) -> Self {
        self.action = Some(SelectAction::Shell(cmdline.into()));
        self
    }

    /// Call a Rust closure with the final [`crate::ModalSelection`] when
    /// this option is picked (runs on the calling thread after `run()`):
    ///
    /// ```no_run
    /// use r_modals::ModalOption;
    /// let opt = ModalOption::new("hi", "Hi").on_select(|sel| println!("picked {}", sel.id));
    /// ```
    pub fn on_select(mut self, f: impl Fn(&crate::ModalSelection) + Send + Sync + 'static) -> Self {
        self.action = Some(SelectAction::Callback(std::sync::Arc::new(f)));
        self
    }

    /// Fallback fired when this option's action fails to spawn (bad binary,
    /// missing shell, …). Only meaningful alongside an action, and only for
    /// process actions — Rust callbacks cannot fail this way:
    ///
    /// ```no_run
    /// use r_modals::ModalOption;
    /// let opt = ModalOption::new("term", "Terminal")
    ///     .command("ghost-terminal")
    ///     .on_error(|sel, err| println!("could not launch {}: {err}", sel.id));
    /// ```
    pub fn on_error(
        mut self,
        f: impl Fn(&crate::ModalSelection, &std::io::Error) + Send + Sync + 'static,
    ) -> Self {
        self.on_error = Some(std::sync::Arc::new(f));
        self
    }

    /// Text haystack used for filtering (label + id + description).
    /// Prefer [`ModalOption::matches_query`] in hot paths: it checks each
    /// field separately and short-circuits, avoiding this allocation.
    pub(crate) fn haystack(&self) -> String {
        let mut s = match &self.description {
            Some(d) => format!("{} {} {}", self.label, self.id, d),
            None => format!("{} {}", self.label, self.id),
        };
        for k in &self.keywords {
            s.push(' ');
            s.push_str(k);
        }
        s
    }

    /// Lowercased haystack, precomputed once per modal for the
    /// case-insensitive fast path (see `ModalApp` filter cache).
    /// Includes descendant text so a parent matches when any of its
    /// children match (it then acts as a gateway to drill into).
    pub(crate) fn haystack_lower(&self) -> String {
        let mut s = self.haystack().to_lowercase();
        for c in &self.children {
            s.push(' ');
            s.push_str(&c.haystack_lower());
        }
        s
    }

    /// Substring match over label + id + description without building the
    /// combined haystack. Case-insensitive callers pass the already-lowered
    /// query; each field is lowered lazily and checked in order so a label
    /// hit skips the id/description work entirely.
    /// A parent also matches when any descendant matches (gateway to drill
    /// into), even if the parent itself is `unsearchable`.
    /// Separators never match a non-empty query.
    pub(crate) fn matches_query(
        &self,
        query: &str,
        query_lower: &str,
        case_sensitive: bool,
    ) -> bool {
        if self.separator {
            return self
                .children
                .iter()
                .any(|c| c.matches_query(query, query_lower, case_sensitive));
        }
        let own = if self.unsearchable {
            false
        } else if case_sensitive {
            self.label.contains(query)
                || self.id.contains(query)
                || self
                    .description
                    .as_deref()
                    .is_some_and(|d| d.contains(query))
                || self.keywords.iter().any(|k| k.contains(query))
        } else {
            // `to_lowercase` allocates; do it field-by-field with early exit
            // so a label match never touches id/description.
            self.label.to_lowercase().contains(query_lower)
                || self.id.to_lowercase().contains(query_lower)
                || self
                    .description
                    .as_deref()
                    .is_some_and(|d| d.to_lowercase().contains(query_lower))
                || self
                    .keywords
                    .iter()
                    .any(|k| k.to_lowercase().contains(query_lower))
        };
        own || self
            .children
            .iter()
            .any(|c| c.matches_query(query, query_lower, case_sensitive))
    }

    /// Fuzzy (subsequence) match over label + id + description + keywords.
    /// Returns the best field score, or `None` when nothing matches.
    /// Parents gateway-match via descendants, like [`ModalOption::matches_query`].
    pub(crate) fn fuzzy_score(&self, query: &str, case_sensitive: bool) -> Option<i32> {
        if self.separator {
            return self
                .children
                .iter()
                .filter_map(|c| c.fuzzy_score(query, case_sensitive))
                .max();
        }
        if self.unsearchable {
            return self
                .children
                .iter()
                .filter_map(|c| c.fuzzy_score(query, case_sensitive))
                .max();
        }
        let mut best: Option<i32> = None;
        let mut consider = |text: &str| {
            if let Some(s) = crate::fuzzy_score(text, query, case_sensitive) {
                best = Some(best.map_or(s, |b: i32| b.max(s)));
            }
        };
        consider(&self.label);
        consider(&self.id);
        if let Some(d) = &self.description {
            consider(d);
        }
        for k in &self.keywords {
            consider(k);
        }
        if let Some(child_best) = self
            .children
            .iter()
            .filter_map(|c| c.fuzzy_score(query, case_sensitive))
            .max()
        {
            // Gateway matches rank below direct matches so the real target
            // sorts first when both are visible at the same level.
            let gated = child_best.saturating_sub(5);
            best = Some(best.map_or(gated, |b: i32| b.max(gated)));
        }
        best
    }
}

/// Index path to the option with `id`, depth-first over submenus.
/// `None` when no option carries that id.
pub(crate) fn find_option_path(opts: &[ModalOption], id: &str) -> Option<Vec<usize>> {
    for (i, o) in opts.iter().enumerate() {
        if o.id == id {
            return Some(vec![i]);
        }
        if let Some(mut child) = find_option_path(&o.children, id) {
            child.insert(0, i);
            return Some(child);
        }
    }
    None
}

/// Mutable reference to the option at `path` (as returned by
/// [`find_option_path`]).
pub(crate) fn option_at_mut<'a>(
    opts: &'a mut [ModalOption],
    path: &[usize],
) -> Option<&'a mut ModalOption> {
    let (&first, rest) = path.split_first()?;
    let opt = opts.get_mut(first)?;
    if rest.is_empty() {
        Some(opt)
    } else {
        option_at_mut(&mut opt.children, rest)
    }
}

#[cfg(test)]
mod tests {
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
        let mut opts =
            vec![ModalOption::new("p", "Parent").sub_option(ModalOption::new("c", "Child"))];
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
        };
        assert!(opt.action.as_ref().unwrap().execute(&sel).is_err());
    }
}
