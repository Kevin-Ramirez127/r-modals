use r_modals::{Layout, Modal, ModalOption};
use std::ffi::OsString;
use std::io::{self, IsTerminal};
use std::path::PathBuf;

fn print_help() {
    println!(
        "r-modals — instant option picker and native dialogs\n\
         \n\
         MODES (the mode must come first)\n\
         \x20  Image demo (default):  r-modals [DIR]\n\
         \x20  Menu:                  <stdin> | r-modals --menu [OPTIONS]\n\
         \x20  Ask:                   r-modals --ask LABEL [OPTIONS]\n\
         \x20  Confirm:               r-modals --confirm TITLE [OPTIONS]\n\
         \x20  Error:                 r-modals --error MESSAGE [OPTIONS]\n\
         \x20  --dmenu and --stdin are aliases of --menu.\n\
         \n\
         COMMON OPTIONS\n\
         \x20  --app-id ID             Wayland app_id / X11 class hint\n\
         \x20  --workspace NAME        open on a compositor workspace\n\
         \x20  --workspace-no-focus    move there without focusing the window\n\
         \x20  --workspace-silent      alias of --workspace-no-focus\n\
         \x20  --workspace-compositor C  auto|hyprland|sway|i3|niri|kde|x11\n\
         \x20  --pid-file PATH         write the window pid for hide/toggle\n\
         \x20  --theme NAME|PATH|CSS   dark|light|transparent|auto,\n\
         \x20                          or a .css file path, or inline CSS\n\
         \x20  --config FILE           unified config.toml (prefixes + shortcuts)\n\
         \x20  --no-title              don't render the title inside the window\n\
         \n\
         MENU OPTIONS\n\
         \x20  --prompt TITLE          window title (default: \"Pick one\")\n\
         \x20  --placeholder TEXT      search placeholder\n\
         \x20  --layout list|grid      default: list\n\
         \x20  --multi                 Space-toggle multi-select, print one pick per line\n\
         \x20  --fuzzy                 subsequence fuzzy match, best-first\n\
         \x20  --case-sensitive        case-sensitive matching\n\
         \x20  --allow-custom          Enter with no match returns the typed text\n\
         \x20  --history-file PATH     persist counts, sort most-used-first\n\
         \x20  --empty-message TEXT    shown when nothing matches\n\
         \x20  --max-results N         cap visible rows (best matches first)\n\
         \x20  --min-match-chars N     hide the full list until N chars are typed\n\
         \x20  --loading-message TEXT  shown while stdin is still open\n\
         \x20  --footer-hint TEXT      hint line at the bottom\n\
         \x20  --no-stream             read all stdin before opening the window\n\
         \x20  --format label|id|tag|json\n\
         \x20                          output shape (default: label); tag prints\n\
         \x20                          the shortcut tag then the label\n\
         \x20  --print-id              alias of --format id\n\
         \x20  --shortcuts FILE        key bindings from a TOML file\n\
         \x20  --no-shortcuts          don't auto-load the default shortcuts file\n\
         \x20  --prefixes FILE         query prefixes from a TOML file\n\
         \x20  --no-prefixes           don't auto-load the default prefixes file\n\
         \n\
         ASK OPTIONS\n\
         \x20  --password              mask the input\n\
         \x20  --initial TEXT          pre-filled value\n\
         \x20  --placeholder TEXT      hint text\n\
         \x20  --message TEXT          body text under the title\n\
         \x20  --confirm-label TEXT    button text (default: OK)\n\
         \n\
         CONFIRM / ERROR OPTIONS\n\
         \x20  --message TEXT          body text under the title\n\
         \x20  --confirm-label TEXT    confirm button text\n\
         \x20  --cancel-label TEXT     cancel button text\n\
         \x20  --title TEXT            window title for --error (default: r-modals)\n\
         \n\
         EXIT CODES\n\
         \x20  0 pick/value/confirmed/shown · 1 cancelled · 2 error\n\
         \n\
         EXAMPLES\n\
         \x20  ls ~/bin | r-modals --menu --prompt Run\n\
         \x20  printf 'a\\nb\\nc' | r-modals --menu --fuzzy --multi\n\
         \x20  printf 'ff\\tFirefox\\nterm\\tTerminal\\n' | r-modals --menu --format json\n\
         \x20  r-modals --ask \"Master password\" --password\n\
         \x20  r-modals --confirm \"Delete?\" --message \"This cannot be undone.\"\n\
         \x20  r-modals --error \"Something went wrong\"\n\
         \x20  r-modals ~/Pictures/wallpapers"
    );
}

/// Image files we treat as pickable.
fn is_image_file(p: &std::path::Path) -> bool {
    matches!(
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp")
    )
}

/// Render monospace ASCII art into an RGBA icon: one character per cell, each
/// expanded `scale`×`scale`, so the binary carries no image files. Non-space
/// characters are drawn in `fg`, spaces in `bg`.
fn ascii_icon(art: &[&str], fg: [u8; 3], bg: [u8; 3], scale: usize) -> (usize, usize, Vec<u8>) {
    let scale = scale.max(1);
    let rows = art.len().max(1);
    let cols = art
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(1)
        .max(1);
    let (w, h) = (cols * scale, rows * scale);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let line: Vec<char> = art
            .get(y / scale)
            .map(|l| l.chars().collect())
            .unwrap_or_default();
        for x in 0..w {
            let lit = line.get(x / scale).is_some_and(|c| *c != ' ');
            let [r, g, b] = if lit { fg } else { bg };
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    (w, h, rgba)
}

/// 8×4 ASCII terminal window for the built-in demo option.
const GLYPH_TERM: [&str; 4] = ["+------+", "| >_   |", "|      |", "+------+"];

/// 5×3 ASCII bitmaps for the colored swatch demo options.
const GLYPH_R: [&str; 5] = ["## ", "# #", "## ", "# #", "# #"];
const GLYPH_G: [&str; 5] = ["###", "#  ", "# #", "# #", "###"];
const GLYPH_B: [&str; 5] = ["## ", "# #", "## ", "# #", "## "];

fn run_image_demo(dir: &str) -> Result<(), String> {
    let mut modal = Modal::new("Pick an image")
        .placeholder("Search images…")
        .layout(Layout::Grid)
        .stylesheet("modal { border-radius: 12px; } tile { width: 128px; height: 128px; }")
        .fit_width_to_content(true)
        .footer_hint("Type to filter · Enter pick · Esc cancel");

    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| is_image_file(p))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files.truncate(30);
    if files.is_empty() {
        eprintln!("no images in {dir:?} — showing built-in choices only");
    }
    for path in &files {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image")
            .to_string();
        modal = modal
            .option(ModalOption::new(format!("file:{}", path.display()), stem).icon(path.clone()));
    }

    // Procedural ASCII-art icons: nothing is embedded or read from disk.
    let (tw, th, tpx) = ascii_icon(&GLYPH_TERM, [225, 228, 235], [28, 28, 34], 4);
    modal = modal.option(
        ModalOption::new("icon:terminal", "Terminal (ASCII icon)")
            .description("Procedural icon, no image file")
            .icon_pixels(tw, th, tpx),
    );

    for (name, glyph, rgb) in [
        ("red", &GLYPH_R, [230, 90, 80]),
        ("green", &GLYPH_G, [80, 190, 110]),
        ("blue", &GLYPH_B, [110, 150, 240]),
    ] {
        let (w, h, px) = ascii_icon(glyph, rgb, [28, 28, 34], 4);
        modal = modal.option(
            ModalOption::new(format!("swatch:{name}"), format!("{name} swatch"))
                .icon_pixels(w, h, px),
        );
    }

    match modal.run()? {
        Some(sel) => println!("picked {} ({}) [index {}]", sel.label, sel.id, sel.index),
        None => println!("cancelled"),
    }
    Ok(())
}

/// Put a dialog mode first so the historical `--prompt X --dmenu` order keeps
/// working even though the child parser expects the mode as argv[1].
fn reorder_mode(args: &[OsString]) -> Option<Vec<OsString>> {
    let index = args.iter().position(|a| {
        matches!(
            a.to_string_lossy().as_ref(),
            "--menu" | "--dmenu" | "--stdin" | "--ask" | "--confirm" | "--error"
        )
    })?;
    let mut out = Vec::with_capacity(args.len());
    out.push(args[index].clone());
    out.extend(args[..index].iter().cloned());
    out.extend(args[index + 1..].iter().cloned());
    Some(out)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return;
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("r-modals {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();
    let dialog_args = if let Some(ordered) = reorder_mode(&raw) {
        Some(ordered)
    } else if !io::stdin().is_terminal() {
        // Piped stdin keeps the historical implicit dmenu mode.
        let mut menu = Vec::with_capacity(raw.len() + 1);
        menu.push(OsString::from("--menu"));
        menu.extend(raw);
        Some(menu)
    } else {
        None
    };
    if let Some(dialog_args) = dialog_args {
        if let Some(code) = r_modals::run_child_args(&dialog_args) {
            std::process::exit(code);
        }
    }

    let dir = args
        .first()
        .filter(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| "assets".to_string());
    if let Err(e) = run_image_demo(&dir) {
        eprintln!("r-modals: {e}");
        std::process::exit(2);
    }
}
