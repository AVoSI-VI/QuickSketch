//! Submission history for one category, oldest to newest.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::{ComboBox, Context, Ui, Vec2};

use crate::persist::{self, SavedState};
use crate::reference::{self, References, TextureCache};

/// The sketch currently shown in the history pop-out window.
pub struct SketchView {
    path: Option<PathBuf>,
    open: Arc<AtomicBool>,
}

impl Default for SketchView {
    fn default() -> Self {
        Self {
            path: None,
            open: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl SketchView {
    fn show_path(&mut self, path: PathBuf) {
        self.path = Some(path);
        self.open.store(true, Ordering::Relaxed);
    }
}

pub fn show(
    ui: &mut Ui,
    refs: &mut References,
    root: &Path,
    state: &mut SavedState,
    view: &mut SketchView,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("History");
        let selected = state.history_category.clone();
        let label = selected
            .as_deref()
            .and_then(|id| state.category(id))
            .map(|category| category.name.clone())
            .unwrap_or_else(|| "Choose category".into());
        let mut picked = None;
        ComboBox::from_id_salt("history-category")
            .selected_text(label)
            .show_ui(ui, |ui| {
                for category in &state.categories {
                    let is_selected = selected.as_deref() == Some(category.id.as_str());
                    if ui.selectable_label(is_selected, &category.name).clicked() && !is_selected {
                        picked = Some(category.id.clone());
                    }
                }
            });
        if let Some(id) = picked {
            state.history_category = Some(id);
            changed = true;
        }
    });

    let Some(id) = state.history_category.clone() else {
        return changed;
    };
    let mut items: Vec<_> = state
        .submissions
        .iter()
        .filter(|item| item.category_id == id)
        .cloned()
        .collect();
    items.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    if items.is_empty() {
        ui.label("No submissions in this category yet.");
        return changed;
    }

    egui::ScrollArea::horizontal()
        .id_salt("history-strip")
        .max_height(130.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for item in items {
                    let path = persist::sketch_path(root, &id, &item.file_name);
                    ui.vertical(|ui| {
                        if let Some(texture) = refs.cache.get(ui.ctx(), &path) {
                            let height = 96.0;
                            let width = (texture.aspect_ratio() * height).clamp(32.0, 180.0);
                            let response = ui.add(
                                egui::Image::from_texture((texture.id(), Vec2::new(width, height)))
                                    .sense(egui::Sense::click()),
                            );
                            if response.clicked() {
                                view.show_path(path);
                            }
                        } else {
                            ui.label("missing");
                        }
                        ui.label(short_date(&item.created_at));
                    });
                }
            });
        });
    changed
}

pub fn show_window(view: &SketchView, cache: &mut TextureCache, ctx: &Context) {
    if !view.open.load(Ordering::Relaxed) {
        return;
    }
    let Some(path) = view.path.clone() else {
        view.open.store(false, Ordering::Relaxed);
        return;
    };
    let texture = cache.get(ctx, &path);
    let title = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Sketch".into());
    reference::show_fitted_image(
        ctx,
        "artproj-sketch",
        title,
        texture,
        Arc::clone(&view.open),
        "Could not load the sketch.",
    );
}

fn short_date(created_at: &str) -> String {
    created_at.chars().take(19).collect()
}
