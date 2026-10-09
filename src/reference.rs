//! File picker, per-category pinned references, and the pop-out window.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use egui::{Context, TextureHandle, Ui, Vec2};

use crate::persist::{self, SavedState};

#[derive(Default)]
pub struct TextureCache {
    images: HashMap<PathBuf, TextureHandle>,
    failed: HashSet<PathBuf>,
}

impl TextureCache {
    pub fn get(&mut self, ctx: &Context, path: &Path) -> Option<TextureHandle> {
        if let Some(texture) = self.images.get(path) {
            return Some(texture.clone());
        }
        if self.failed.contains(path) {
            return None;
        }
        let Some(image) = load_color(path) else {
            self.failed.insert(path.to_path_buf());
            return None;
        };
        let texture = ctx.load_texture(
            path.display().to_string(),
            image,
            egui::TextureOptions::LINEAR,
        );
        self.images.insert(path.to_path_buf(), texture.clone());
        Some(texture)
    }
}

pub struct References {
    current: Option<PathBuf>,
    open: Arc<AtomicBool>,
    pub cache: TextureCache,
    /// File dialog running off the UI thread so the window keeps responding.
    pending_pick: Option<Receiver<Option<PathBuf>>>,
}

impl Default for References {
    fn default() -> Self {
        Self {
            current: None,
            open: Arc::new(AtomicBool::new(false)),
            cache: TextureCache::default(),
            pending_pick: None,
        }
    }
}

pub struct RefResult {
    pub status: Option<String>,
    pub dirty: bool,
}

pub fn bar(ui: &mut Ui, refs: &mut References, root: &Path, state: &mut SavedState) -> RefResult {
    let mut result = RefResult {
        status: None,
        dirty: false,
    };
    if let Some(status) = take_picked_image(refs, ui.ctx()) {
        result.status = Some(status);
    }
    let picking = refs.pending_pick.is_some();
    ui.horizontal(|ui| {
        let open = egui::Button::new(if picking {
            "Opening…"
        } else {
            "Open reference"
        });
        if ui.add_enabled(!picking, open).clicked() {
            start_pick(refs, ui.ctx());
        }
        if ui.button("Pin to category").clicked() {
            result = pin_current(refs, root, state);
        }
    });
    let active = state.active_category.clone();
    let pinned = active
        .as_deref()
        .and_then(|id| state.category(id))
        .map(|category| category.pinned_refs.clone());
    if let Some(names) = pinned.filter(|names| !names.is_empty()) {
        let id = active.unwrap_or_default();
        egui::ScrollArea::horizontal()
            .id_salt("pinned-refs")
            .max_height(56.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for name in names {
                        let path = persist::ref_path(root, &id, &name);
                        if thumb(ui, refs, &path).clicked() {
                            result.status = refs.open_path(ui.ctx(), path);
                        }
                    }
                });
            });
    }
    result
}

pub fn show_window(refs: &mut References, ctx: &Context) {
    if !refs.open.load(Ordering::Relaxed) {
        return;
    }
    let Some(path) = refs.current.clone() else {
        refs.open.store(false, Ordering::Relaxed);
        return;
    };
    let texture = refs.cache.get(ctx, &path);
    let open = Arc::clone(&refs.open);
    let title = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Reference".into());
    ctx.show_viewport_deferred(
        egui::ViewportId::from_hash_of("artproj-reference"),
        egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([720.0, 720.0]),
        move |ui, _class| {
            if ui.input(|input| input.viewport().close_requested()) {
                open.store(false, Ordering::Relaxed);
            }
            let Some(texture) = &texture else {
                ui.label("Could not load the reference image.");
                return;
            };
            let available = ui.available_size();
            let sized = texture.size_vec2();
            let scale = if sized.x > 0.0 && sized.y > 0.0 {
                (available.x / sized.x).min(available.y / sized.y).max(0.01)
            } else {
                1.0
            };
            ui.image((texture.id(), sized * scale));
        },
    );
}

fn thumb(ui: &mut Ui, refs: &mut References, path: &Path) -> egui::Response {
    if let Some(texture) = refs.cache.get(ui.ctx(), path) {
        let response = ui.add(
            egui::Image::from_texture((texture.id(), texture.size_vec2()))
                .fit_to_exact_size(Vec2::new(48.0, 48.0))
                .sense(egui::Sense::click()),
        );
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        response.on_hover_text(name)
    } else {
        ui.button("missing")
            .on_hover_text(path.display().to_string())
    }
}

fn pin_current(refs: &References, root: &Path, state: &mut SavedState) -> RefResult {
    let Some(source) = refs.current.clone() else {
        return RefResult {
            status: Some("Open a reference image first.".into()),
            dirty: false,
        };
    };
    let Some(id) = state.active_category.clone() else {
        return RefResult {
            status: Some("Pick a category before pinning.".into()),
            dirty: false,
        };
    };
    let Some(category) = state.category_mut(&id) else {
        return RefResult {
            status: Some("Pick a category before pinning.".into()),
            dirty: false,
        };
    };
    match copy_pin(root, category, &source) {
        Ok((message, dirty)) => RefResult {
            status: Some(message),
            dirty,
        },
        Err(message) => RefResult {
            status: Some(message),
            dirty: false,
        },
    }
}

fn copy_pin(
    root: &Path,
    category: &mut crate::persist::Category,
    source: &Path,
) -> Result<(String, bool), String> {
    let safe = safe_file_name(source);
    if category.pinned_refs.iter().any(|name| name == &safe) {
        return Ok((format!("Already pinned {safe}"), false));
    }
    let dir = persist::ref_dir(root, &category.id);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let stored = unique_name(&dir, &safe);
    fs::copy(source, dir.join(&stored)).map_err(|err| err.to_string())?;
    category.pinned_refs.push(stored.clone());
    Ok((format!("Pinned {stored}"), true))
}

fn start_pick(refs: &mut References, ctx: &Context) {
    if refs.pending_pick.is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let path = rfd::FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
            .pick_file();
        let _ = tx.send(path);
    });
    refs.pending_pick = Some(rx);
    ctx.request_repaint_after(Duration::from_millis(100));
}

fn take_picked_image(refs: &mut References, ctx: &Context) -> Option<String> {
    let received = refs.pending_pick.as_ref()?.try_recv();
    match received {
        Ok(Some(path)) => {
            refs.pending_pick = None;
            refs.open_path(ctx, path)
        }
        Ok(None) => {
            refs.pending_pick = None;
            None
        }
        Err(TryRecvError::Empty) => {
            ctx.request_repaint_after(Duration::from_millis(100));
            None
        }
        Err(TryRecvError::Disconnected) => {
            refs.pending_pick = None;
            Some("Could not open the file dialog.".into())
        }
    }
}

impl References {
    fn open_path(&mut self, ctx: &Context, path: PathBuf) -> Option<String> {
        if self.cache.get(ctx, &path).is_none() {
            return Some(format!("Could not open {}", path.display()));
        }
        self.current = Some(path);
        self.open.store(true, Ordering::Relaxed);
        None
    }
}

fn load_color(path: &Path) -> Option<egui::ColorImage> {
    let image = image::open(path).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        image.as_raw(),
    ))
}

fn safe_file_name(source: &Path) -> String {
    let raw = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("reference.png");
    let safe: String = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() || safe.starts_with('.') {
        format!("reference{safe}")
    } else {
        safe
    }
}

fn unique_name(dir: &Path, file_name: &str) -> String {
    if !dir.join(file_name).exists() {
        return file_name.to_string();
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("reference");
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("png");
    for index in 2..10_000 {
        let candidate = format!("{stem}-{index}.{ext}");
        if !dir.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{stem}-copy.{ext}")
}
