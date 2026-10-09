//! Typed errors from [`crate::Modal::run`] and friends.

/// Failure from showing a modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalError {
    /// The modal had no options to show.
    NoOptions,
    /// A stylesheet failed to parse.
    Css(String),
    /// The GUI backend failed to start (e.g. no display).
    Backend(String),
}

impl std::fmt::Display for ModalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModalError::NoOptions => write!(f, "quick-modals: no options provided"),
            ModalError::Css(e) => write!(f, "quick-modals CSS error: {e}"),
            ModalError::Backend(e) => write!(f, "quick-modals backend error: {e}"),
        }
    }
}

impl std::error::Error for ModalError {}

/// Lets existing `fn main() -> Result<(), String>` call sites keep using `?`.
impl From<ModalError> for String {
    fn from(e: ModalError) -> Self {
        e.to_string()
    }
}
