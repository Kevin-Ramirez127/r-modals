//! `quick-modals` — instant modals for Rust.
//!
//! A tiny framework to pop a centered, searchable option picker:
//!
//! ```no_run
//! use quick_modals::{Modal, ModalOption, Theme};
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
//!
//! Styling is done with CSS stylesheets: [`Modal::stylesheet`] or
//! [`Theme::from_css`] (selectors `modal`, `tile`, `tile[selected]`,
//! `tile description`; see `Theme::from_css` for the full subset).
//!
//! Backend: [`eframe`]/[`egui`] borderless always-on-top window.
//! This approximates on X11/Wayland without requiring
//! compositor-specific layer-shell protocols.

mod app;
mod css;
mod modal;
mod option;
mod search;
mod theme;

pub use modal::{Modal, ModalSelection};
pub use option::{ErrorAction, IconSource, ModalOption, SelectAction};
pub use search::{filter_options, SearchConfig};
pub use theme::{Layout, Position, Theme};
