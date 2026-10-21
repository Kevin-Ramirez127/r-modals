//! Shared viewport/eframe setup for every dialog type.

use crate::{ModalError, Position, Theme};

/// Everything needed to open one borderless, always-on-top window.
pub(crate) struct WindowSpec {
    pub(crate) title: String,
    pub(crate) app_id: String,
    pub(crate) inner_size: [f32; 2],
    pub(crate) min_inner_size: [f32; 2],
    pub(crate) resizable: bool,
    pub(crate) position: Position,
}

impl WindowSpec {
    /// Full-size picker window driven by the theme.
    pub(crate) fn modal(title: String, theme: &Theme, app_id: String, position: Position) -> Self {
        Self {
            title,
            app_id,
            inner_size: [theme.width, theme.height],
            min_inner_size: [320.0, 200.0],
            resizable: true,
            position,
        }
    }

    /// Compact dialog window (`Confirm`/`Prompt`) capped at `max_width`.
    pub(crate) fn dialog(
        title: String,
        theme: &Theme,
        app_id: String,
        max_width: f32,
        height: f32,
        position: Position,
    ) -> Self {
        Self {
            title,
            app_id,
            inner_size: [theme.width.min(max_width), height],
            min_inner_size: [320.0, 150.0],
            resizable: false,
            position,
        }
    }

    fn viewport(&self) -> egui::ViewportBuilder {
        let mut v = egui::ViewportBuilder::default()
            .with_title(&self.title)
            .with_app_id(&self.app_id)
            .with_inner_size(self.inner_size)
            .with_min_inner_size(self.min_inner_size)
            .with_decorations(false)
            .with_always_on_top()
            .with_resizable(self.resizable)
            .with_transparent(true);
        match self.position {
            Position::Center => {}
            Position::TopCenter { y_offset } => {
                // Best-effort: eframe centers by default; the offset must be
                // applied by the compositor/WM.
                let _ = y_offset;
            }
            Position::Custom { x, y } => {
                v = v.with_position([x as f32, y as f32]);
            }
        }
        v
    }
}

/// Run one dialog app to completion.
pub(crate) fn run_app<A: eframe::App + 'static>(
    spec: WindowSpec,
    create: impl FnOnce(&eframe::CreationContext<'_>) -> A + 'static,
) -> Result<(), ModalError> {
    let title = spec.title.clone();
    let options = eframe::NativeOptions {
        viewport: spec.viewport(),
        ..Default::default()
    };
    eframe::run_native(
        &title,
        options,
        Box::new(move |cc| Ok(Box::new(create(cc)) as Box<dyn eframe::App>)),
    )
    .map_err(|e| ModalError::Backend(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::WindowSpec;
    use crate::{Position, Theme};

    #[test]
    fn modal_uses_the_theme_size() {
        let theme = Theme::dark();
        let spec = WindowSpec::modal("t".into(), &theme, "app".into(), Position::Center);
        assert_eq!(spec.inner_size, [theme.width, theme.height]);
        assert!(spec.resizable);
    }

    #[test]
    fn dialogs_are_capped_and_fixed() {
        let theme = Theme::dark();
        let confirm = WindowSpec::dialog(
            "t".into(),
            &theme,
            "app".into(),
            420.0,
            180.0,
            Position::Center,
        );
        let prompt = WindowSpec::dialog(
            "t".into(),
            &theme,
            "app".into(),
            480.0,
            190.0,
            Position::Center,
        );
        assert_eq!(confirm.inner_size, [theme.width.min(420.0), 180.0]);
        assert_eq!(prompt.inner_size, [theme.width.min(480.0), 190.0]);
        assert!(!confirm.resizable);
        assert!(!prompt.resizable);
    }
}
