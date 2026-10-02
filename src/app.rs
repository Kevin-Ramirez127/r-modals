use crate::{filter_options, IconSource, Modal, ModalSelection};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub(crate) struct ModalApp {
    modal: Modal,
    query: String,
    cursor: usize,
    textures: HashMap<String, egui::TextureHandle>,
    result: Arc<Mutex<Option<ModalSelection>>>,
    search_id: egui::Id,
    focused_once: bool,
}

impl ModalApp {
    pub(crate) fn new(
        _cc: &eframe::CreationContext<'_>,
        modal: Modal,
        result: Arc<Mutex<Option<ModalSelection>>>,
    ) -> Self {
        let query = modal.initial_query.clone();
        Self {
            modal,
            query,
            cursor: 0,
            textures: HashMap::new(),
            result,
            search_id: egui::Id::new("quick-modals-search"),
            focused_once: false,
        }
    }

    fn visible_indices(&self) -> Vec<usize> {
        if !self.modal.search.enabled {
            return (0..self.modal.options.len()).collect();
        }
        filter_options(
            &self.modal.options,
            &self.query,
            self.modal.search.case_sensitive,
        )
    }

    fn choose(&self, visible: &[usize], pos: usize) -> Option<ModalSelection> {
        let &opt_idx = visible.get(pos)?;
        let opt = &self.modal.options[opt_idx];
        Some(ModalSelection {
            id: opt.id.clone(),
            label: opt.label.clone(),
            index: opt_idx,
            query: self.query.clone(),
        })
    }

    fn finish(&self, sel: Option<ModalSelection>, ctx: &egui::Context) {
        if let Ok(mut g) = self.result.lock() {
            *g = sel;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn icon_texture(
        &mut self,
        ctx: &egui::Context,
        option_id: &str,
        icon: &IconSource,
    ) -> Option<egui::TextureHandle> {
        let key = icon.cache_key(option_id)?;
        if let Some(t) = self.textures.get(&key) {
            return Some(t.clone());
        }
        let img = load_icon(icon)?;
        let tex = ctx.load_texture(&key, img, egui::TextureOptions::LINEAR);
        self.textures.insert(key.clone(), tex.clone());
        Some(tex)
    }
}

fn load_icon(icon: &IconSource) -> Option<egui::ColorImage> {
    match icon {
        IconSource::None => None,
        IconSource::File(p) => {
            let bytes = std::fs::read(p).ok()?;
            decode_bytes(&bytes)
        }
        IconSource::Bytes(b) => decode_bytes(b),
        IconSource::Pixels {
            width,
            height,
            rgba,
        } => {
            if rgba.len() != width * height * 4 {
                return None;
            }
            Some(egui::ColorImage::from_rgba_premultiplied(
                [*width, *height],
                rgba,
            ))
        }
    }
}

fn decode_bytes(bytes: &[u8]) -> Option<egui::ColorImage> {
    let dyn_img = image::load_from_memory(bytes).ok()?;
    let rgba = dyn_img.to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [w, h],
        rgba.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_valid_png() {
        let bytes = include_bytes!("../assets/firefox.png");
        assert!(decode_bytes(bytes).is_some());
        let img = decode_bytes(bytes).unwrap();
        assert_eq!(img.size, [32, 32]);
    }

    #[test]
    fn decode_invalid_bytes_returns_none() {
        assert!(decode_bytes(b"not an image").is_none());
        assert!(decode_bytes(include_bytes!("../README.md")).is_none());
    }

    #[test]
    fn load_icon_variants() {
        // File (ships with repo)
        assert!(load_icon(&IconSource::File("assets/firefox.png".into())).is_some());
        // Missing file -> None (graceful)
        assert!(load_icon(&IconSource::File("assets/does-not-exist.png".into())).is_none());
        // Bytes
        let bytes = include_bytes!("../assets/terminal.png").to_vec();
        assert!(load_icon(&IconSource::Bytes(bytes)).is_some());
        assert!(load_icon(&IconSource::Bytes(b"junk".to_vec())).is_none());
        // Pixels: valid + length mismatch
        assert!(load_icon(&IconSource::Pixels {
            width: 2,
            height: 2,
            rgba: vec![255; 2 * 2 * 4],
        })
        .is_some());
        assert!(load_icon(&IconSource::Pixels {
            width: 2,
            height: 2,
            rgba: vec![255; 3],
        })
        .is_none());
        assert!(load_icon(&IconSource::None).is_none());
    }
}

impl eframe::App for ModalApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let t = self.modal.theme.clone();

        // Global keys.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.finish(None, ctx);
            return;
        }

        let visible = self.visible_indices();
        if self.cursor >= visible.len() {
            self.cursor = visible.len().saturating_sub(1);
        }

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
            && !visible.is_empty()
        {
            self.cursor = (self.cursor + 1) % visible.len();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
            && !visible.is_empty()
        {
            self.cursor = self.cursor.checked_sub(1).unwrap_or(visible.len() - 1);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            let sel = self.choose(&visible, self.cursor);
            self.finish(sel, ctx);
            return;
        }

        // Styling.
        let mut style = (*ctx.style()).clone();
        style.visuals.window_fill = t.background;
        style.visuals.panel_fill = t.background;
        style.visuals.selection.bg_fill = t.selected;
        style.visuals.widgets.inactive.bg_fill = t.surface;
        style.visuals.widgets.hovered.bg_fill = t.selected.linear_multiply(0.85);
        style.visuals.widgets.active.bg_fill = t.selected;
        ctx.set_style(style);

        let frame = egui::Frame::new()
            .fill(t.background)
            .corner_radius(t.corner_radius)
            .stroke(egui::Stroke::new(t.border_width, t.border))
            .inner_margin(t.padding)
            .outer_margin(8.0);

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            ui.style_mut().override_text_style = Some(egui::TextStyle::Body);
            // Search field.
            if self.modal.search.enabled {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .id(self.search_id)
                        .hint_text(&self.modal.search.placeholder)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Vec2::new(10.0, 8.0)),
                );
                if (self.modal.search.autofocus && !self.focused_once)
                    || self.query.is_empty() && !self.focused_once
                {
                    resp.request_focus();
                    self.focused_once = true;
                }
                if resp.changed() {
                    self.cursor = 0;
                }
                ui.add_space(8.0);
            } else if self.modal.search.autofocus && !self.focused_once {
                // No search field: nothing to focus; mark done so we don't retry.
                self.focused_once = true;
            }

            let visible = self.visible_indices();
            if visible.is_empty() {
                ui.colored_label(t.dim_text, "No matches");
                return;
            }

            egui::ScrollArea::vertical()
                .max_height(t.height - 110.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = t.item_spacing;
                    for (pos, &opt_idx) in visible.iter().enumerate() {
                        let opt = &self.modal.options[opt_idx].clone();
                        let selected = pos == self.cursor;
                        let row_fill = if selected { t.selected } else { t.surface };

                        let row = egui::Frame::new()
                            .fill(row_fill)
                            .corner_radius(t.corner_radius * 0.6)
                            .inner_margin(egui::Margin::symmetric(8, 6))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    // Icon (cached texture or placeholder box).
                                    let tex = self.icon_texture(ctx, &opt.id, &opt.icon);
                                    if let Some(tex) = tex {
                                        ui.add(
                                            egui::Image::new(&tex)
                                                .max_size(egui::Vec2::splat(t.icon_size))
                                                .corner_radius(4.0),
                                        );
                                    } else if !matches!(opt.icon, IconSource::None) {
                                        // Broken image path: small placeholder so rows align.
                                        let (r, _) = ui.allocate_exact_size(
                                            egui::Vec2::splat(t.icon_size),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(
                                            r,
                                            4.0,
                                            t.background.linear_multiply(1.4),
                                        );
                                    }
                                    ui.vertical(|ui| {
                                        let mut rt = egui::RichText::new(&opt.label)
                                            .size(t.font_size)
                                            .color(t.text);
                                        if selected {
                                            rt = rt.strong();
                                        }
                                        ui.label(rt);
                                        if t.show_descriptions {
                                            if let Some(d) = &opt.description {
                                                ui.label(
                                                    egui::RichText::new(d)
                                                        .size(t.font_size * 0.82)
                                                        .color(t.dim_text),
                                                );
                                            }
                                        }
                                    });
                                });
                            });

                        // Click + hover select.
                        if row.response.hovered() {
                            self.cursor = pos;
                        }
                        if row.response.clicked() {
                            let sel = self.choose(&visible, pos);
                            self.finish(sel, ctx);
                            return;
                        }
                    }
                });
        });

        // Keep repainting for focus / texture readiness.
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}
