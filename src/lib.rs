//! `r-modals` — instant modals for Rust.
//!
//! A tiny framework to pop a centered, searchable option picker:
//!
//! ```no_run
//! use r_modals::{Modal, ModalOption, Theme};
//!
//! let choice = Modal::new("Launcher")
//!     .placeholder("Type to filter…")
//!     .case_sensitive(false)
//!     .option(ModalOption::new("ff", "Firefox").description("Web browser"))
//!     .option(ModalOption::new("term", "Terminal").description("Alacritty"))
//!     .theme(Theme::dark())
//!     .run()
//!     .unwrap();
//!
//! if let Some(sel) = choice {
//!     println!("picked {} ({})", sel.label, sel.id);
//! }
//! ```
//!
//! Modules:
//! - [`Modal`] — builder + [`Modal::run`] entry point.
//! - [`ModalOption`] / [`IconSource`] — options + images.
//! - [`SearchConfig`] + [`filter_options`] — testable filtering logic.
//! - [`Theme`] / [`Position`] / [`Layout`] — customization.
//! - [`ShortcutSpec`] — file-configurable key bindings ([`Modal::shortcuts_file`]).
//! - [`PrefixSpec`] — query prefixes that run an action on confirm
//!   ([`Modal::prefixes_file`]).
//! - [`Confirm`] / [`Prompt`] — one-shot dialogs built on the same backend.
//! - [`Dialog`] / [`DialogSession`] / [`DialogOutput`] — spawn the consumer's
//!   own binary in `--menu`/`--ask`/`--confirm`/`--error` mode; call
//!   [`dialog_main!`] first in `main`.
//! - [`ConfigFile`] — unified prefix/shortcut config with legacy migration.
//! - [`ModalError`] — typed errors from [`Modal::run`].
//!
//! Styling is done with CSS stylesheets: [`Modal::stylesheet`] or
//! [`Theme::from_css`] (selectors `modal`, `tile`, `tile[selected]`,
//! `tile description`; see `Theme::from_css` for the full subset).
//!
//! Backend: [`eframe`]/[`egui`] borderless always-on-top window.
//! This approximates on X11/Wayland without requiring
//! compositor-specific layer-shell protocols.

mod app;
mod config;
mod confirm;
mod css;
mod dialog;
mod error;
mod modal;
mod multi;
mod option;
mod prefix;
mod prompt;
mod search;
mod shortcut;
mod theme;
mod window;

pub use config::ConfigFile;
pub use confirm::Confirm;
pub use dialog::{run_child_args, run_child_from_args, Dialog, DialogOutput, DialogSession};
pub use error::ModalError;
pub use modal::{Modal, ModalSelection};
pub use multi::{run_multi, MultiSelection};
pub use option::{ErrorAction, IconSource, ModalOption, SelectAction};
pub use prefix::{default_prefixes_path, load_prefixes_file, parse_prefixes, PrefixSpec};
pub use prompt::Prompt;
pub use search::{filter_options, filter_options_with, fuzzy_score, SearchConfig};
pub use shortcut::{default_shortcuts_path, load_shortcuts_file, parse_shortcuts, ShortcutSpec};
pub use theme::{Layout, Position, Theme};
