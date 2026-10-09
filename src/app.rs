//! Window layout. Panels call the shared stroke, category, and file helpers.

use std::path::PathBuf;

use crate::canvas::{Canvas, Tool};
use crate::category;
use crate::history;
use crate::persist::{self, SavedState};
use crate::reference::{self, References};
use crate::submit::SubmitPhase;
use crate::timer::SketchTimer;

pub struct SketchApp {
    root: PathBuf,
    state: SavedState,
    canvas: Canvas,
    timer: SketchTimer,
    submit: SubmitPhase,
    refs: References,
    name_edit: String,
    status: String,
}

impl SketchApp {
    pub fn new() -> Self {
        let root = persist::data_dir();
        let _ = std::fs::create_dir_all(&root);
        let state = persist::load(&root);
        let name_edit = state
            .active_category
            .as_deref()
            .and_then(|id| state.category(id))
            .map(|category| category.name.clone())
            .unwrap_or_default();
        let status = format!("Saving to {}", root.display());
        let app = Self {
            root,
            state,
            canvas: Canvas::default(),
            timer: SketchTimer::default(),
            submit: SubmitPhase::default(),
            refs: References::default(),
            name_edit,
            status,
        };
        let _ = persist::save(&app.root, &app.state);
        app
    }

    fn remember(&mut self) {
        if let Err(err) = persist::save(&self.root, &self.state) {
            self.status = format!("Could not save state: {err}");
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        self.timer.show(ui);
        let action = category::show(
            ui,
            &mut self.state,
            &mut self.name_edit,
            self.submit.is_placing(),
            self.submit.can_finish(),
        );
        if action.changed {
            self.remember();
        }
        if action.start {
            self.start_submission();
        }
        if action.cancel {
            self.submit.cancel();
        }
        if action.finish {
            self.finish_submission();
        }
        self.canvas.brush_ui(ui);
        let pinned = reference::bar(ui, &mut self.refs, &self.root, &mut self.state);
        if pinned.dirty {
            self.remember();
        }
        if let Some(status) = pinned.status {
            self.status = status;
        }
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
    }

    fn start_submission(&mut self) {
        let Some(id) = self.state.active_category.clone() else {
            self.status = "Add a category first.".into();
            return;
        };
        self.canvas.commit();
        self.submit.begin(id);
        self.status = "Drag a circle around the sketch.".into();
    }

    fn finish_submission(&mut self) {
        self.canvas.commit();
        match self.submit.finish(
            &self.root,
            self.canvas.strokes_mut(),
            &mut self.state.submissions,
        ) {
            Ok(path) => {
                self.status = format!("Saved {path}");
                self.remember();
            }
            Err(err) => self.status = err,
        }
    }
}

impl eframe::App for SketchApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.timer.tick() {
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        let mut history_changed = false;
        egui::Panel::bottom("history").show(ui, |ui| {
            history_changed = history::show(ui, &mut self.refs, &self.root, &mut self.state);
        });
        if history_changed {
            self.remember();
        }
        let tool = if self.submit.is_placing() {
            Tool::Circle(self.submit.circle())
        } else {
            Tool::Brush
        };
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(circle) = self.canvas.show(ui, &tool) {
                self.submit.set_circle(circle);
            }
        });
        reference::show_window(&mut self.refs, ui.ctx());
    }

    fn on_exit(&mut self) {
        let _ = persist::save(&self.root, &self.state);
    }
}
