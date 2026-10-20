//! Multi-select picker: Space toggles, Enter confirms the set.

use crate::{filter_options_with, Modal, ModalError, ModalSelection, Position};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// One pick inside a multi-select result. Alias of [`ModalSelection`].
pub type MultiSelection = ModalSelection;

/// Run `modal` (forced into multi mode) and collect every toggled pick.
///
/// Space toggles the highlighted row, Enter confirms, Esc cancels
/// (empty vec). Skips disabled/separator rows. Actions fire once per
/// pick, in order, after the window closes.
pub fn run_multi(modal: Modal) -> Result<Vec<ModalSelection>, ModalError> {
    if modal.options.is_empty() {
        return Err(ModalError::NoOptions);
    }
    let mut modal = modal.multi_select(true);
    modal.apply_history();
    modal.theme = modal.resolved_theme()?;
    let history_file = modal.history_file.clone();
    let actions: Vec<(Vec<usize>, crate::SelectAction, Option<crate::ErrorAction>)> = {
        // Same walk as `Modal::collect_actions` (flat paths: multi-select
        // does not drill into submenus — parents toggle as whole rows).
        let mut out = Vec::new();
        for (i, o) in modal.options.iter().enumerate() {
            if let Some(a) = &o.action {
                out.push((vec![i], a.clone(), o.on_error.clone()));
            }
        }
        out
    };

    let shared: Arc<Mutex<Vec<ModalSelection>>> = Arc::new(Mutex::new(Vec::new()));
    let inner = shared.clone();
    let title = modal.title.clone();
    let theme = modal.theme.clone();
    let position = modal.position;
    let app_id = modal.app_id.clone();

    let viewport = {
        let mut v = egui::ViewportBuilder::default()
            .with_title(&title)
            .with_app_id(&app_id)
            .with_inner_size([theme.width, theme.height])
            .with_min_inner_size([320.0, 200.0])
            .with_decorations(false)
            .with_always_on_top()
            .with_resizable(true)
            .with_transparent(true);
        match position {
            Position::Center => {}
            Position::TopCenter { y_offset } => {
                let _ = y_offset;
            }
            Position::Custom { x, y } => {
                v = v.with_position([x as f32, y as f32]);
            }
        }
        v
    };

    let res = eframe::run_native(
        &title,
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |_cc| {
            struct MultiApp {
                modal: Modal,
                query: String,
                cursor: usize,
                picked: HashSet<usize>,
                result: Arc<Mutex<Vec<ModalSelection>>>,
                search_id: egui::Id,
                focused: bool,
            }
            impl eframe::App for MultiApp {
                // See ModalApp::clear_color: keep the margin ring transparent.
                fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
                    [0.0, 0.0, 0.0, 0.0]
                }

                fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
                    let t = self.modal.theme.clone();
                    let (esc, down, up, enter, space) = ctx.input_mut(|i| {
                        (
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Space),
                        )
                    });
                    let mut visible: Vec<usize> = if !self.modal.search.enabled {
                        (0..self.modal.options.len()).collect()
                    } else {
                        filter_options_with(&self.modal.options, &self.query, &self.modal.search)
                    };
                    if let Some(m) = self.modal.max_results {
                        visible.truncate(m);
                    }
                    if self.cursor >= visible.len() {
                        self.cursor = visible.len().saturating_sub(1);
                    }
                    if esc {
                        if let Ok(mut g) = self.result.lock() {
                            *g = Vec::new();
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        return;
                    }
                    if down && !visible.is_empty() {
                        self.cursor = (self.cursor + 1) % visible.len();
                    }
                    if up && !visible.is_empty() {
                        self.cursor = self.cursor.checked_sub(1).unwrap_or(visible.len() - 1);
                    }
                    let toggle = |app: &mut MultiApp| {
                        if let Some(&idx) = visible.get(app.cursor) {
                            if app
                                .modal
                                .options
                                .get(idx)
                                .is_some_and(|o| o.activatable() && !o.has_children())
                                && !app.picked.insert(idx)
                            {
                                app.picked.remove(&idx);
                            }
                        }
                    };
                    if space {
                        toggle(self);
                    }
                    if enter {
                        let mut out: Vec<ModalSelection> = self
                            .picked
                            .iter()
                            .map(|&idx| {
                                let o = &self.modal.options[idx];
                                ModalSelection {
                                    id: o.id.clone(),
                                    label: o.label.clone(),
                                    index: idx,
                                    path: vec![idx],
                                    query: self.query.clone(),
                                    custom_input: false,
                                }
                            })
                            .collect();
                        out.sort_by_key(|s| s.index);
                        if let Ok(mut g) = self.result.lock() {
                            *g = out;
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        return;
                    }
                    let frame = egui::Frame::new()
                        .fill(t.background)
                        .corner_radius(t.corner_radius)
                        .inner_margin(t.padding)
                        .outer_margin(t.margin);
                    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                        if self.modal.search.enabled {
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut self.query)
                                    .id(self.search_id)
                                    .hint_text(&self.modal.search.placeholder)
                                    .desired_width(f32::INFINITY),
                            );
                            if !self.focused {
                                resp.request_focus();
                                self.focused = true;
                            }
                            if resp.changed() {
                                self.cursor = 0;
                            }
                            ui.add_space(8.0);
                        }
                        let visible_now: Vec<usize> = if !self.modal.search.enabled {
                            (0..self.modal.options.len()).collect()
                        } else {
                            let mut v = filter_options_with(
                                &self.modal.options,
                                &self.query,
                                &self.modal.search,
                            );
                            if let Some(m) = self.modal.max_results {
                                v.truncate(m);
                            }
                            v
                        };
                        egui::ScrollArea::vertical()
                            .max_height(t.height - 140.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                for (pos, &idx) in visible_now.iter().enumerate() {
                                    let o = &self.modal.options[idx];
                                    let selected = pos == self.cursor;
                                    let checked = self.picked.contains(&idx);
                                    let fill = if selected { t.selected } else { t.surface };
                                    let resp = egui::Frame::new()
                                        .fill(fill)
                                        .corner_radius(6.0)
                                        .inner_margin(egui::Margin::symmetric(8, 6))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.horizontal(|ui| {
                                                let mark = if checked { "☑" } else { "☐" };
                                                ui.label(
                                                    egui::RichText::new(mark)
                                                        .size(t.font_size)
                                                        .color(t.dim_text),
                                                );
                                                let col =
                                                    if o.disabled { t.dim_text } else { t.text };
                                                ui.label(
                                                    egui::RichText::new(&o.label)
                                                        .size(t.font_size)
                                                        .color(col),
                                                );
                                            });
                                        })
                                        .response;
                                    if resp.clicked() {
                                        self.cursor = pos;
                                        if o.activatable()
                                            && !o.has_children()
                                            && !self.picked.insert(idx)
                                        {
                                            self.picked.remove(&idx);
                                        }
                                    }
                                }
                            });
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("Space toggle · Enter confirm · Esc cancel")
                                .size(t.font_size * 0.78)
                                .color(t.dim_text),
                        );
                    });
                }
            }
            Ok(Box::new(MultiApp {
                modal,
                query: String::new(),
                cursor: 0,
                picked: HashSet::new(),
                result: inner,
                search_id: egui::Id::new("r-modals-multi"),
                focused: false,
            }) as Box<dyn eframe::App>)
        }),
    );
    res.map_err(|e| ModalError::Backend(e.to_string()))?;
    let picks: Vec<ModalSelection> = shared.lock().map(|g| g.clone()).unwrap_or_default();
    for sel in &picks {
        if let Some(path) = &history_file {
            Modal::new("")
                .history_file(path.clone())
                .record_history(&sel.id);
        }
        if let Some((_, action, on_error)) = actions.iter().find(|(p, _, _)| *p == sel.path) {
            if let Err(e) = action.execute(sel) {
                if let Some(f) = on_error {
                    f(sel, &e);
                }
            }
        }
    }
    Ok(picks)
}
