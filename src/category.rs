//! User-editable sketch categories and the submission buttons beside them.

use egui::{ComboBox, Ui};

use crate::persist::SavedState;

pub struct CategoryAction {
    pub changed: bool,
    pub start: bool,
    pub finish: bool,
    pub cancel: bool,
}

pub fn show(
    ui: &mut Ui,
    state: &mut SavedState,
    name_edit: &mut String,
    placing: bool,
    can_finish: bool,
) -> CategoryAction {
    let mut action = CategoryAction {
        changed: false,
        start: false,
        finish: false,
        cancel: false,
    };
    ui.horizontal(|ui| {
        let selected = state.active_category.clone();
        let label = selected
            .as_ref()
            .and_then(|id| state.category(id))
            .map(|category| category.name.as_str())
            .unwrap_or("Category");
        let mut picked = None;
        ComboBox::from_id_salt("active-category")
            .selected_text(label)
            .show_ui(ui, |ui| {
                for category in &state.categories {
                    let is_selected = selected.as_deref() == Some(category.id.as_str());
                    if ui.selectable_label(is_selected, &category.name).clicked() && !is_selected {
                        picked = Some((category.id.clone(), category.name.clone()));
                    }
                }
            });
        if let Some((id, name)) = picked {
            state.active_category = Some(id);
            *name_edit = name;
            action.changed = true;
        }

        if let Some(id) = state.active_category.clone() {
            ui.label(format!("{} submitted", state.count(&id)));
        }

        ui.add(
            egui::TextEdit::singleline(name_edit)
                .desired_width(140.0)
                .hint_text("Category name"),
        );
        if ui.button("Add").clicked() {
            add_category(state, name_edit);
            action.changed = true;
        }
        if ui.button("Rename").clicked() && rename_category(state, name_edit) {
            action.changed = true;
        }
        if ui.button("Delete").clicked() && delete_category(state, name_edit) {
            action.changed = true;
        }
        if ui.button("Start submission").clicked() {
            action.start = true;
        }
        if ui
            .add_enabled(placing && can_finish, egui::Button::new("Finish"))
            .clicked()
        {
            action.finish = true;
        }
        if ui
            .add_enabled(placing, egui::Button::new("Cancel"))
            .clicked()
        {
            action.cancel = true;
        }
    });
    if placing {
        ui.label("Drag on the canvas to size a circle around the sketch, then Finish.");
    }
    action
}

fn add_category(state: &mut SavedState, name_edit: &mut String) {
    let name = cleaned_name(name_edit);
    let id = state.alloc_id();
    state.categories.push(crate::persist::Category {
        id: id.clone(),
        name,
        pinned_refs: Vec::new(),
    });
    state.active_category = Some(id);
    if let Some(category) = state
        .active_category
        .as_deref()
        .and_then(|id| state.category(id))
    {
        *name_edit = category.name.clone();
    }
}

fn rename_category(state: &mut SavedState, name_edit: &str) -> bool {
    let Some(id) = state.active_category.clone() else {
        return false;
    };
    let Some(category) = state.category_mut(&id) else {
        return false;
    };
    category.name = cleaned_name(name_edit);
    true
}

fn delete_category(state: &mut SavedState, name_edit: &mut String) -> bool {
    let Some(id) = state.active_category.clone() else {
        return false;
    };
    state.categories.retain(|category| category.id != id);
    state.submissions.retain(|item| item.category_id != id);
    if state.history_category.as_deref() == Some(id.as_str()) {
        state.history_category = None;
    }
    if state.categories.is_empty() {
        *state = crate::persist::SavedState::default();
    }
    state.active_category = state.categories.first().map(|category| category.id.clone());
    if let Some(category) = state
        .active_category
        .as_deref()
        .and_then(|id| state.category(id))
    {
        *name_edit = category.name.clone();
    }
    true
}

fn cleaned_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "Untitled".into()
    } else {
        trimmed.to_string()
    }
}
