use std::collections::BTreeMap;
use std::fs;

use serde::{Deserialize, Serialize};

use super::{LIBRARY_FILE, ProjectStore, now, read_json_opt, write_json};
use crate::{CoreError, Result};

/// A folder on the home screen. Folders are virtual: project directories stay flat under the projects
/// root, named by id, and the library file says where each one is shown.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
}

/// Kept beside the project directories so moving or trashing a project never touches its files or modified time.
#[derive(Serialize, Deserialize, Default)]
pub(super) struct Library {
    #[serde(default)]
    folders: Vec<Folder>,
    #[serde(default)]
    pub(super) placement: BTreeMap<String, String>,
    #[serde(default)]
    pub(super) trash: BTreeMap<String, String>,
}

impl Library {
    pub(super) fn has(&self, folder: &str) -> bool {
        self.folders.iter().any(|f| f.id == folder)
    }

    fn subtree(&self, folder: &str) -> Vec<String> {
        let mut all = vec![folder.to_string()];
        let mut i = 0;
        while i < all.len() {
            let parent = all[i].clone();
            let children: Vec<String> = self
                .folders
                .iter()
                .filter(|f| f.parent.as_deref() == Some(parent.as_str()) && !all.contains(&f.id))
                .map(|f| f.id.clone())
                .collect();
            all.extend(children);
            i += 1;
        }
        all
    }
}

impl ProjectStore {
    pub(super) fn library(&self) -> Result<Library> {
        Ok(read_json_opt(&self.root.join(LIBRARY_FILE))?.unwrap_or_default())
    }

    pub(super) fn save_library(&self, library: &Library) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        write_json(&self.root.join(LIBRARY_FILE), library)
    }

    pub fn folders(&self) -> Result<Vec<Folder>> {
        Ok(self.library()?.folders)
    }

    pub fn create_folder(&self, name: &str, parent: Option<&str>) -> Result<Folder> {
        let mut library = self.library()?;
        if let Some(parent) = parent.filter(|p| !library.has(p)) {
            return Err(folder_not_found(parent));
        }
        let folder = Folder {
            id: uuid::Uuid::new_v4().simple().to_string(),
            name: folder_name(name)?,
            parent: parent.map(str::to_string),
        };
        library.folders.push(folder.clone());
        self.save_library(&library)?;
        Ok(folder)
    }

    pub fn rename_folder(&self, id: &str, name: &str) -> Result<()> {
        let mut library = self.library()?;
        let name = folder_name(name)?;
        let folder = library
            .folders
            .iter_mut()
            .find(|f| f.id == id)
            .ok_or_else(|| folder_not_found(id))?;
        folder.name = name;
        self.save_library(&library)
    }

    /// Projects inside go to the trash; returns their ids.
    pub fn delete_folder(&self, id: &str) -> Result<Vec<String>> {
        let mut library = self.library()?;
        if !library.has(id) {
            return Err(folder_not_found(id));
        }
        let gone = library.subtree(id);
        let inside: Vec<String> = library
            .placement
            .iter()
            .filter(|(_, folder)| gone.contains(folder))
            .map(|(project, _)| project.clone())
            .collect();
        let ts = now();
        for project in &inside {
            library.placement.remove(project);
            library.trash.entry(project.clone()).or_insert_with(|| ts.clone());
        }
        library.folders.retain(|f| !gone.contains(&f.id));
        self.save_library(&library)?;
        Ok(inside)
    }

    pub fn move_project(&self, id: &str, folder: Option<&str>) -> Result<()> {
        self.project_dir(id)?;
        let mut library = self.library()?;
        match folder {
            Some(folder) if library.has(folder) => {
                library.placement.insert(id.to_string(), folder.to_string());
            }
            Some(folder) => return Err(folder_not_found(folder)),
            None => {
                library.placement.remove(id);
            }
        }
        self.save_library(&library)
    }

    pub fn trash(&self, id: &str) -> Result<()> {
        self.project_dir(id)?;
        let mut library = self.library()?;
        library.trash.entry(id.to_string()).or_insert_with(now);
        self.save_library(&library)
    }

    pub fn restore(&self, id: &str) -> Result<()> {
        let mut library = self.library()?;
        library.trash.remove(id);
        self.save_library(&library)
    }

    pub fn empty_trash(&self) -> Result<Vec<String>> {
        let trashed: Vec<String> = self.library()?.trash.into_keys().collect();
        for id in &trashed {
            match self.delete(id) {
                Ok(()) | Err(CoreError::ProjectNotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        let mut library = self.library()?;
        library.trash.clear();
        self.save_library(&library)?;
        Ok(trashed)
    }
}

fn folder_not_found(id: &str) -> CoreError {
    CoreError::invalid(format!("folder {id} not found"))
}

fn folder_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CoreError::invalid("a folder needs a name"));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use crate::fixtures::TempRoot;

    #[test]
    fn folders_and_trash_place_projects_without_touching_them() {
        let root = TempRoot::new();
        let store = root.store();
        let a = store.create("가", 100.0, 50.0).expect("create").project;
        let b = store.create("나", 100.0, 50.0).expect("create").project;
        let find = |id: &str| {
            store
                .list()
                .expect("list")
                .into_iter()
                .find(|p| p.id == id)
                .expect("listed")
        };
        assert_eq!(
            (find(&a.id).folder, find(&a.id).deleted_at),
            (None, None),
            "top level by default"
        );

        let year = store.create_folder(" 2026 ", None).expect("folder");
        let autumn = store
            .create_folder("하반기", Some(&year.id))
            .expect("nested folder");
        assert_eq!(year.name, "2026", "names are trimmed");
        assert!(store.create_folder("  ", None).is_err(), "a folder needs a name");
        assert!(store.create_folder("고아", Some("nope")).is_err());
        store.move_project(&a.id, Some(&autumn.id)).expect("move");
        assert!(store.move_project(&a.id, Some("nope")).is_err());
        assert_eq!(find(&a.id).folder.as_deref(), Some(autumn.id.as_str()));
        assert_eq!(
            find(&a.id).updated_at,
            a.updated_at,
            "moving leaves the project's own files alone"
        );
        store.rename_folder(&autumn.id, "가을").expect("rename");
        assert_eq!(store.folders().expect("folders")[1].name, "가을");

        store.trash(&b.id).expect("trash");
        assert!(find(&b.id).deleted_at.is_some());
        assert!(
            root.0.join(&b.id).join("project.json").is_file(),
            "the files stay until the trash is emptied"
        );
        store.restore(&b.id).expect("restore");
        assert_eq!(find(&b.id).deleted_at, None);

        let inside = store.delete_folder(&year.id).expect("delete folder");
        assert_eq!(
            inside,
            std::slice::from_ref(&a.id),
            "projects in nested folders go to the trash"
        );
        assert!(
            store.folders().expect("folders").is_empty(),
            "nested folders go with it"
        );
        assert_eq!(
            (find(&a.id).folder, find(&a.id).deleted_at.is_some()),
            (None, true)
        );

        assert_eq!(store.empty_trash().expect("empty"), std::slice::from_ref(&a.id));
        assert!(!root.0.join(&a.id).exists());
        assert_eq!(
            store.list().expect("list").len(),
            1,
            "only the project outside the trash is left"
        );
    }
}
