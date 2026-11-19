//! Live search-query reactions.
//!
//! [`Modal::on_query_change`](crate::Modal::on_query_change) installs a
//! closure that runs whenever the text in the search field changes. The
//! closure can then swap the option list (for example, switch from apps to
//! browser tabs when the query starts with `>t`) and choose the text the
//! list is filtered by, all while the modal stays open.
//!
//! ```no_run
//! use r_modals::{Modal, ModalOption, QueryChange};
//!
//! let apps = vec![ModalOption::new("ff", "Firefox")];
//! let tabs = vec![ModalOption::new("https://example.com", "Example")];
//! let apps = Some(apps);
//!
//! let picked = Modal::new("Launcher")
//!     .option(ModalOption::new("ff", "Firefox"))
//!     .on_query_change(move |query| {
//!         if let Some(rest) = query.strip_prefix(">t") {
//!             Some(QueryChange::new().options(tabs.clone()).filter(rest.trim()))
//!         } else {
//!             apps.clone()
//!                 .map(|options| QueryChange::new().options(options).filter(query))
//!         }
//!     })
//!     .run()
//!     .unwrap();
//! ```

use crate::ModalOption;
use std::fmt;
use std::sync::{Arc, Mutex};

/// How the modal should react to the current query text.
///
/// Returned from the closure registered with
/// [`Modal::on_query_change`](crate::Modal::on_query_change). Every field is
/// optional; unset fields keep their current value. Returning `None` from the
/// closure (or an empty `QueryChange`) means "no changes" and filters by the
/// raw query text.
///
/// Replacing `options` clears the current submenu level and resets the cursor
/// to the top row. The replacement list's actions are executed on pick,
/// exactly like options supplied before the window opened.
#[derive(Clone, Debug, Default)]
pub struct QueryChange {
    /// Replace the option list (a mode switch).
    pub options: Option<Vec<ModalOption>>,
    /// Text the visible list is filtered by. Defaults to the raw query, so a
    /// mode prefix such as `>t` can be hidden from the filter.
    pub filter: Option<String>,
    /// Replace the search placeholder. An empty string clears it.
    pub placeholder: Option<String>,
    /// Replace the window title. An empty string is ignored.
    pub title: Option<String>,
    /// Replace the message shown when the filter matches nothing.
    pub empty_message: Option<String>,
    /// Replace the bottom hint line. An empty string clears it.
    pub footer_hint: Option<String>,
    /// Show an in-place loading state in the option area.
    ///
    /// While set, the picker keeps the search field editable but replaces the
    /// rows with a centered spinner and message. The hook is then called again
    /// about 20 times per second, giving it a chance to finish the work it
    /// started and return the options; supplying options clears the state.
    pub loading: Option<bool>,
    /// Message shown by the in-place loading state. Unset keeps the current
    /// text (the modal-level `loading_message`, or "Loading…").
    pub loading_message: Option<String>,
}

impl QueryChange {
    /// No changes: keep the current list and filter by the raw query.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the option list (a mode switch).
    pub fn options(mut self, options: Vec<ModalOption>) -> Self {
        self.options = Some(options);
        self
    }

    /// Filter the list by this text instead of the raw query.
    pub fn filter(mut self, filter: impl Into<String>) -> Self {
        self.filter = Some(filter.into());
        self
    }

    /// Replace the search placeholder.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Replace the window title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Replace the empty-state message.
    pub fn empty_message(mut self, message: impl Into<String>) -> Self {
        self.empty_message = Some(message.into());
        self
    }

    /// Replace the bottom hint line.
    pub fn footer_hint(mut self, hint: impl Into<String>) -> Self {
        self.footer_hint = Some(hint.into());
        self
    }

    /// Show (or stop showing) the in-place loading state.
    ///
    /// While set, the picker keeps the search field editable but replaces the
    /// rows with a centered spinner and message. The hook is then called again
    /// about 20 times per second, giving it a chance to finish the work it
    /// started and return the options; supplying options clears the state.
    ///
    /// ```no_run
    /// use r_modals::{Modal, ModalOption, QueryChange};
    ///
    /// let modal = Modal::new("Launcher")
    ///     .options(vec![ModalOption::new("a", "App")])
    ///     .on_query_change(|query| {
    ///         query.starts_with(">t").then(|| {
    ///             QueryChange::new()
    ///                 .filter("")
    ///                 .loading(true)
    ///                 .loading_message("Loading open tabs…")
    ///         })
    ///     });
    /// ```
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = Some(loading);
        self
    }

    /// Set the message shown by the in-place loading state.
    pub fn loading_message(mut self, message: impl Into<String>) -> Self {
        self.loading_message = Some(message.into());
        self
    }
}

/// The closure installed by [`Modal::on_query_change`](crate::Modal::on_query_change).
///
/// Wrapped in `Arc<Mutex<_>>` so [`Modal`](crate::Modal) keeps its `Clone`
/// and `Debug` derives; a cloned modal shares the same hook state.
type HookFn = dyn FnMut(&str) -> Option<QueryChange> + Send;

#[derive(Clone)]
pub(crate) struct QueryHook(Arc<Mutex<Box<HookFn>>>);

impl QueryHook {
    pub(crate) fn new<F>(hook: F) -> Self
    where
        F: FnMut(&str) -> Option<QueryChange> + Send + 'static,
    {
        Self(Arc::new(Mutex::new(Box::new(hook))))
    }

    /// Run the hook. A poisoned lock (a panicking hook) is treated as "no
    /// changes" instead of aborting the picker.
    pub(crate) fn call(&self, query: &str) -> Option<QueryChange> {
        self.0.lock().ok().and_then(|mut hook| hook(query))
    }
}

impl fmt::Debug for QueryHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("QueryHook(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builders_set_each_field() {
        let change = QueryChange::new()
            .options(vec![ModalOption::new("a", "A")])
            .filter("a")
            .placeholder("Search…")
            .title("Tabs")
            .empty_message("Nothing")
            .footer_hint("Enter")
            .loading(true)
            .loading_message("Loading…");
        assert_eq!(change.options.as_ref().map(Vec::len), Some(1));
        assert_eq!(change.filter.as_deref(), Some("a"));
        assert_eq!(change.placeholder.as_deref(), Some("Search…"));
        assert_eq!(change.title.as_deref(), Some("Tabs"));
        assert_eq!(change.empty_message.as_deref(), Some("Nothing"));
        assert_eq!(change.footer_hint.as_deref(), Some("Enter"));
        assert_eq!(change.loading, Some(true));
        assert_eq!(change.loading_message.as_deref(), Some("Loading…"));
    }

    #[test]
    fn hook_runs_and_shares_state_across_clones() {
        let mut seen = Vec::new();
        let hook = QueryHook::new(move |query: &str| {
            seen.push(query.to_string());
            Some(QueryChange::new().filter(query.to_uppercase()))
        });
        let clone = hook.clone();
        let first = hook.call("ab").unwrap();
        let second = clone.call("cd").unwrap();
        assert_eq!(first.filter.as_deref(), Some("AB"));
        assert_eq!(second.filter.as_deref(), Some("CD"));
    }

    #[test]
    fn hook_debug_is_readable() {
        let hook = QueryHook::new(|_| None);
        assert_eq!(format!("{hook:?}"), "QueryHook(..)");
    }
}
