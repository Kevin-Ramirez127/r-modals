//! Benchmark: CSS-styled theme vs plain struct theme.
//!
//! Run with: `cargo run --release --example css_bench`
//!
//! Measures the one-time cost of parsing/applying a stylesheet
//! (`Theme::from_css`) against building the same theme with a struct
//! literal (no CSS). Per-frame rendering is identical in both
//! cases: the stylesheet is resolved once in `Modal::run`, before
//! the window opens.

use quick_modals::Theme;
use std::hint::black_box;
use std::time::Instant;

fn bench(name: &str, iters: u32, mut f: impl FnMut()) {
    for _ in 0..10 {
        f();
    }
    let start = Instant::now();
    for _ in 0..iters {
        f();
    }
    let dt = start.elapsed();
    let ns = dt.as_secs_f64() * 1e9 / iters as f64;
    if ns >= 10_000.0 {
        println!("{name:42} {us:10.2} µs/op", us = ns / 1000.0);
    } else {
        println!("{name:42} {ns:10.1} ns/op");
    }
}

fn main() {
    let css = include_str!("../assets/theme.css");

    // One-shot cost: what an app actually pays once at startup.
    let start = Instant::now();
    let theme = Theme::from_css(black_box(css)).unwrap();
    println!(
        "one-shot Theme::from_css (theme.css): {:.2} µs\n",
        start.elapsed().as_secs_f64() * 1e6
    );

    bench("Theme::from_css (theme.css)", 500, || {
        black_box(Theme::from_css(black_box(css)).unwrap());
    });

    // A 200-rule stylesheet: parse cost scales with input size.
    let mut big = String::new();
    for i in 0..200 {
        big.push_str(&format!(
            "modal {{ width: {}px; }}\ntile[selected] {{ background: #112233; }}\n",
            500 + i
        ));
    }
    bench("Theme::from_css (200 rules)", 100, || {
        black_box(Theme::from_css(black_box(&big)).unwrap());
    });

    // Baseline without CSS: the equivalent struct literal.
    bench("Theme struct literal (no CSS)", 100_000, || {
        black_box(Theme {
            width: 520.0,
            corner_radius: 14.0,
            padding: 14.0,
            margin: 8.0,
            font_size: 15.0,
            border: egui::Color32::from_rgb(65, 72, 104),
            border_width: 1.0,
            accent: egui::Color32::from_rgb(122, 162, 247),
            tile_size: 128.0,
            ..Theme::dark()
        });
    });

    // Keep the compiler honest: the CSS theme has the expected values.
    assert_eq!(theme.width, 520.0);
    println!("\nNote: per-frame rendering cost is identical — the stylesheet");
    println!("is resolved once in Modal::run; update() only reads Theme fields.");
}
