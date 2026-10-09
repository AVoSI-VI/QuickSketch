//! App data directory and `state.json`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Category {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub pinned_refs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Submission {
    pub category_id: String,
    pub file_name: String,
    /// RFC3339 timestamp. These sort in submission order.
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedState {
    #[serde(default = "one")]
    pub next_id: u64,
    pub categories: Vec<Category>,
    #[serde(default)]
    pub submissions: Vec<Submission>,
    pub active_category: Option<String>,
    #[serde(default)]
    pub history_category: Option<String>,
}

fn one() -> u64 {
    1
}

impl Default for SavedState {
    fn default() -> Self {
        Self {
            next_id: 1,
            categories: vec![Category {
                id: "c1".into(),
                name: "Sketches".into(),
                pinned_refs: Vec::new(),
            }],
            submissions: Vec::new(),
            active_category: Some("c1".into()),
            history_category: None,
        }
    }
}

impl SavedState {
    pub fn alloc_id(&mut self) -> String {
        self.next_id += 1;
        format!("c{}", self.next_id)
    }

    pub fn category(&self, id: &str) -> Option<&Category> {
        self.categories.iter().find(|category| category.id == id)
    }

    pub fn category_mut(&mut self, id: &str) -> Option<&mut Category> {
        self.categories
            .iter_mut()
            .find(|category| category.id == id)
    }

    pub fn count(&self, id: &str) -> usize {
        self.submissions
            .iter()
            .filter(|item| item.category_id == id)
            .count()
    }
}

pub fn data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "artproj", "artproj")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("artproj-data"))
}

pub fn load(root: &Path) -> SavedState {
    let path = root.join("state.json");
    let Ok(bytes) = fs::read(&path) else {
        return SavedState::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save(root: &Path, state: &SavedState) -> std::io::Result<()> {
    fs::create_dir_all(root)?;
    let path = root.join("state.json");
    let tmp = root.join("state.json.tmp");
    let bytes = serde_json::to_vec_pretty(state).map_err(std::io::Error::other)?;
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

pub fn sketch_dir(root: &Path, category_id: &str) -> PathBuf {
    root.join("sketches").join(category_id)
}

pub fn ref_dir(root: &Path, category_id: &str) -> PathBuf {
    root.join("refs").join(category_id)
}

pub fn sketch_path(root: &Path, category_id: &str, file_name: &str) -> PathBuf {
    sketch_dir(root, category_id).join(file_name)
}

pub fn ref_path(root: &Path, category_id: &str, file_name: &str) -> PathBuf {
    ref_dir(root, category_id).join(file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_state() {
        let root = std::env::temp_dir().join(format!("artproj-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut state = SavedState::default();
        let id = state.alloc_id();
        state.categories.push(Category {
            id: id.clone(),
            name: "Figures".into(),
            pinned_refs: vec!["hand.png".into()],
        });
        state.submissions.push(Submission {
            category_id: id.clone(),
            file_name: "a.png".into(),
            created_at: "2026-10-08T00:00:00Z".into(),
        });
        state.active_category = Some(id);
        save(&root, &state).unwrap();
        let loaded = load(&root);
        assert_eq!(loaded, state);
        let _ = fs::remove_dir_all(&root);
    }
}
