use r_modals::{Layout, Modal, ModalOption, Theme};
use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;

fn print_help() {
    println!(
        "r-modals — instant option picker\n\
         \n\
         MODES\n\
         \x20  Image demo (default):  r-modals [DIR]\n\
         \x20  dmenu mode:            <stdin> | r-modals --dmenu [OPTIONS]\n\
         \x20                         (stdin piped in also implies dmenu mode)\n\
         \n\
         DMENU OPTIONS\n\
         \x20  --prompt TITLE         window title (default: \"Pick one\")\n\
         \x20  --placeholder TEXT     search placeholder\n\
         \x20  --layout list|grid     default: list\n\
         \x20  --theme NAME|PATH|CSS  dark|light|transparent|auto (default: dark),\n\
         \x20                         or a .css file path, or inline CSS\n\
         \x20  --multi                Space-toggle multi-select, print one pick per line\n\
         \x20  --fuzzy                subsequence fuzzy match, best-first\n\
         \x20  --case-sensitive       case-sensitive matching\n\
         \x20  --allow-custom         Enter with no match returns the typed text\n\
         \x20  --history-file PATH    persist counts, sort most-used-first\n\
         \x20  --empty-message TEXT   shown when nothing matches\n\
         \x20  --max-results N        cap visible rows (best matches first)\n\
         \x20  --print-id             print option id instead of label\n\
         \x20  --shortcuts FILE       key bindings from a TOML file (bind a key to\n\
         \x20                         an option id, or run a command/shell)\n\
         \x20  --no-shortcuts         don't auto-load ~/.config/r-modals/shortcuts.toml\n\
         \x20  --prefixes FILE        query prefixes from a TOML file (type the\n\
         \x20                         prefix, Enter runs its command/shell)\n\
         \x20  --no-prefixes          don't auto-load ~/.config/r-modals/prefixes.toml\n\
         \x20  -h, --help             this help\n\
         \n\
         EXIT CODES\n\
         \x20  0 pick printed · 1 cancelled (no output) · 2 error\n\
         \n\
         EXAMPLES\n\
         \x20  ls ~/bin | r-modals --dmenu --prompt Run\n\
         \x20  printf 'a\\nb\\nc' | r-modals --dmenu --fuzzy --multi\n\
         \x20  ls ~/bin | r-modals --dmenu --shortcuts ~/.config/r-modals/shortcuts.toml\n\
         \x20  ls ~/bin | r-modals --dmenu --prefixes ~/.config/r-modals/prefixes.toml\n\
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

#[derive(Default)]
struct DmenuOpts {
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
    print_id: bool,
    shortcuts: Option<String>,
    no_shortcuts: bool,
    prefixes: Option<String>,
    no_prefixes: bool,
}

fn resolve_theme(spec: Option<&str>) -> Result<(Theme, Option<String>), String> {
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
            // A real file path wins; otherwise treat it as inline CSS.
            if std::path::Path::new(other).is_file() {
                let css = std::fs::read_to_string(other)
                    .map_err(|e| format!("cannot read theme file {other:?}: {e}"))?;
                return Ok((Theme::dark(), Some(css)));
            }
            Ok((Theme::dark(), Some(other.to_string())))
        }
    }
}

fn run_dmenu(opts: DmenuOpts) -> i32 {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("r-modals: cannot read stdin");
        return 2;
    }
    let lines: Vec<String> = input
        .lines()
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        eprintln!("r-modals: no options on stdin");
        return 2;
    }
    let (theme, stylesheet) = match resolve_theme(opts.theme.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("r-modals: {e}");
            return 2;
        }
    };
    let layout = match opts.layout.as_deref() {
        Some("grid") => Layout::Grid,
        _ => Layout::List,
    };
    let mut modal = Modal::new(opts.prompt.unwrap_or_else(|| "Pick one".to_string()))
        .theme(theme)
        .layout(layout)
        .fuzzy(opts.fuzzy)
        .case_sensitive(opts.case_sensitive)
        .allow_custom_input(opts.allow_custom)
        .footer_hint("Enter pick · Esc cancel");
    if let Some(p) = opts.placeholder {
        modal = modal.placeholder(p);
    }
    if let Some(css) = stylesheet {
        modal = modal.stylesheet(css);
    }
    if let Some(h) = opts.history_file {
        modal = modal.history_file(h);
    }
    if let Some(m) = opts.empty_message {
        modal = modal.empty_message(m);
    }
    if let Some(m) = opts.max_results {
        modal = modal.max_results(m);
    }
    // Explicit --shortcuts FILE wins; --no-shortcuts disables the default file.
    if let Some(path) = opts.shortcuts {
        modal = modal.shortcuts_file(path);
    } else if opts.no_shortcuts {
        modal = modal.auto_shortcuts(false);
    }
    // Same contract for query prefixes.
    if let Some(path) = opts.prefixes {
        modal = modal.prefixes_file(path);
    } else if opts.no_prefixes {
        modal = modal.auto_prefixes(false);
    }
    for line in &lines {
        modal = modal.option(ModalOption::new(line.clone(), line.clone()));
    }
    if opts.multi {
        match modal.run_multi() {
            Ok(picks) if picks.is_empty() => 1,
            Ok(picks) => {
                for s in &picks {
                    if opts.print_id {
                        println!("{}", s.id);
                    } else {
                        println!("{}", s.label);
                    }
                }
                0
            }
            Err(e) => {
                eprintln!("r-modals: {e}");
                2
            }
        }
    } else {
        match modal.run() {
            Ok(Some(sel)) => {
                if opts.print_id {
                    println!("{}", sel.id);
                } else {
                    println!("{}", sel.label);
                }
                0
            }
            Ok(None) => 1,
            Err(e) => {
                eprintln!("r-modals: {e}");
                2
            }
        }
    }
}

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
    let stdin_piped = !io::stdin().is_terminal();
    let mut dmenu = stdin_piped;
    let mut opts = DmenuOpts::default();
    let mut dir: Option<String> = None;
    let mut i = 0;
    let mut bad: Option<String> = None;
    while i < args.len() {
        let a = args[i].as_str();
        let next = |i: &mut usize| -> Option<String> {
            *i += 1;
            args.get(*i).cloned()
        };
        match a {
            "--dmenu" | "--stdin" => dmenu = true,
            "--prompt" => opts.prompt = next(&mut i),
            "--placeholder" => opts.placeholder = next(&mut i),
            "--layout" => opts.layout = next(&mut i),
            "--theme" => opts.theme = next(&mut i),
            "--multi" => opts.multi = true,
            "--fuzzy" => opts.fuzzy = true,
            "--case-sensitive" => opts.case_sensitive = true,
            "--allow-custom" => opts.allow_custom = true,
            "--history-file" => opts.history_file = next(&mut i),
            "--empty-message" => opts.empty_message = next(&mut i),
            "--max-results" => match next(&mut i).as_deref().map(str::parse::<usize>) {
                Some(Ok(n)) => opts.max_results = Some(n),
                _ => bad = Some("--max-results needs a number".to_string()),
            },
            "--print-id" => opts.print_id = true,
            "--shortcuts" => opts.shortcuts = next(&mut i),
            "--no-shortcuts" => opts.no_shortcuts = true,
            "--prefixes" => opts.prefixes = next(&mut i),
            "--no-prefixes" => opts.no_prefixes = true,
            s if s.starts_with('-') => bad = Some(format!("unknown flag {s} (see --help)")),
            s => {
                if dir.is_none() && !dmenu {
                    dir = Some(s.to_string());
                }
            }
        }
        i += 1;
    }
    if let Some(e) = bad {
        eprintln!("r-modals: {e}");
        std::process::exit(2);
    }
    if dmenu {
        std::process::exit(run_dmenu(opts));
    }
    let dir = dir.unwrap_or_else(|| "assets".to_string());
    if let Err(e) = run_image_demo(&dir) {
        eprintln!("r-modals: {e}");
        std::process::exit(2);
    }
}
