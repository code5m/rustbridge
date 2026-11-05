//! Multi-document state: each open Rust file owns its source, saved baseline,
//! cursor, mode and disk identity independently of the currently visible editor.
use crate::{file_watch::FileStamp, persistence::Journal};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct Document {
    pub id: u64,
    pub path: PathBuf,
    pub source: String,
    pub saved_source: String,
    /// Cached dirty bit, changed only when source or saved baseline changes.
    pub dirty: bool,
    pub cursor_byte: usize,
    pub mode: String,
    pub disk_stamp: Option<FileStamp>,
    pub external_change: bool,
    pub pending_recovery: Option<Arc<Journal>>,
}

impl Document {
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub struct DocumentUpdate {
    pub source: String,
    pub saved_source: String,
    pub cursor_byte: usize,
    pub mode: String,
    pub disk_stamp: Option<FileStamp>,
    pub external_change: bool,
}

#[derive(Default)]
pub struct DocumentManager {
    documents: Vec<Document>,
    active_id: Option<u64>,
    next_id: u64,
}

fn same_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}

impl DocumentManager {
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Document> {
        self.documents.iter()
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.documents.iter().map(|doc| doc.path.clone()).collect()
    }

    pub fn dirty_paths(&self) -> Vec<PathBuf> {
        self.documents
            .iter()
            .filter(|doc| doc.is_dirty())
            .map(|doc| doc.path.clone())
            .collect()
    }

    pub fn dirty_count(&self) -> usize {
        self.documents.iter().filter(|doc| doc.is_dirty()).count()
    }

    pub fn recovery_count(&self) -> usize {
        self.documents
            .iter()
            .filter(|doc| doc.pending_recovery.is_some())
            .count()
    }

    pub fn set_pending_recovery(&mut self, path: &Path, journal: Option<Journal>) {
        if let Some(doc) = self.get_mut(path) {
            doc.pending_recovery = journal.map(Arc::new);
        }
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.documents.iter().any(|doc| same_path(&doc.path, path))
    }

    pub fn get(&self, path: &Path) -> Option<&Document> {
        self.documents.iter().find(|doc| same_path(&doc.path, path))
    }

    pub fn get_mut(&mut self, path: &Path) -> Option<&mut Document> {
        self.documents
            .iter_mut()
            .find(|doc| same_path(&doc.path, path))
    }

    pub fn active(&self) -> Option<&Document> {
        self.documents
            .iter()
            .find(|doc| Some(doc.id) == self.active_id)
    }

    pub fn active_path(&self) -> Option<PathBuf> {
        self.active().map(|doc| doc.path.clone())
    }

    pub fn insert_loaded(
        &mut self,
        path: PathBuf,
        source: String,
        cursor_byte: usize,
        mode: String,
        disk_stamp: Option<FileStamp>,
    ) -> u64 {
        if let Some(existing) = self
            .documents
            .iter()
            .find(|doc| same_path(&doc.path, &path))
        {
            return existing.id;
        }
        self.next_id = self.next_id.saturating_add(1).max(1);
        let id = self.next_id;
        self.documents.push(Document {
            id,
            path,
            saved_source: source.clone(),
            source,
            dirty: false,
            cursor_byte,
            mode,
            disk_stamp,
            external_change: false,
            pending_recovery: None,
        });
        id
    }

    pub fn activate(&mut self, path: &Path) -> Option<Document> {
        let doc = self
            .documents
            .iter()
            .find(|doc| same_path(&doc.path, path))?;
        self.active_id = Some(doc.id);
        Some(doc.clone())
    }

    #[cfg(test)]
    pub fn update(&mut self, path: &Path, update: DocumentUpdate) {
        if let Some(doc) = self.get_mut(path) {
            doc.source = update.source;
            doc.saved_source = update.saved_source;
            doc.dirty = doc.source != doc.saved_source;
            doc.cursor_byte = update.cursor_byte;
            doc.mode = update.mode;
            doc.disk_stamp = update.disk_stamp;
            doc.external_change = update.external_change;
        }
    }

    pub fn update_active_source(
        &mut self,
        source: String,
        saved_source: String,
        cursor_byte: usize,
        mode: String,
        disk_stamp: Option<FileStamp>,
        external_change: bool,
    ) {
        let Some(active_id) = self.active_id else {
            return;
        };
        if let Some(doc) = self.documents.iter_mut().find(|doc| doc.id == active_id) {
            doc.source = source;
            doc.saved_source = saved_source;
            doc.dirty = doc.source != doc.saved_source;
            doc.cursor_byte = cursor_byte;
            doc.mode = mode;
            doc.disk_stamp = disk_stamp;
            doc.external_change = external_change;
        }
    }

    pub fn update_active_metadata(
        &mut self,
        cursor_byte: usize,
        mode: String,
        disk_stamp: Option<FileStamp>,
        external_change: bool,
    ) {
        let Some(active_id) = self.active_id else {
            return;
        };
        if let Some(doc) = self.documents.iter_mut().find(|doc| doc.id == active_id) {
            doc.cursor_byte = cursor_byte;
            doc.mode = mode;
            doc.disk_stamp = disk_stamp;
            doc.external_change = external_change;
        }
    }

    pub fn update_saved(
        &mut self,
        path: &Path,
        saved_source: String,
        disk_stamp: Option<FileStamp>,
    ) -> bool {
        let Some(doc) = self.get_mut(path) else {
            return false;
        };
        doc.saved_source = saved_source;
        doc.dirty = doc.source != doc.saved_source;
        doc.disk_stamp = disk_stamp;
        doc.external_change = false;
        true
    }

    pub fn set_external_change(&mut self, path: &Path, changed: bool) {
        if let Some(doc) = self.get_mut(path) {
            doc.external_change = changed;
        }
    }

    pub fn remove(&mut self, path: &Path) -> Option<Document> {
        let index = self
            .documents
            .iter()
            .position(|doc| same_path(&doc.path, path))?;
        let removed = self.documents.remove(index);
        if self.active_id == Some(removed.id) {
            self.active_id = self
                .documents
                .get(index.min(self.documents.len().saturating_sub(1)))
                .or_else(|| {
                    index
                        .checked_sub(1)
                        .and_then(|previous| self.documents.get(previous))
                })
                .map(|doc| doc.id);
        }
        Some(removed)
    }

    pub fn clear(&mut self) -> Vec<Document> {
        self.active_id = None;
        std::mem::take(&mut self.documents)
    }

    pub fn set_active_path(&mut self, path: &Path) -> bool {
        let Some(doc) = self.documents.iter().find(|doc| same_path(&doc.path, path)) else {
            return false;
        };
        self.active_id = Some(doc.id);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_keep_independent_buffers_dirty_states_and_identity() {
        let mut manager = DocumentManager::default();
        let a = PathBuf::from("/tmp/rustbridge-document-a.rs");
        let b = PathBuf::from("/tmp/rustbridge-document-b.rs");
        manager.insert_loaded(a.clone(), "fn a() {}".into(), 0, "rust".into(), None);
        manager.insert_loaded(b.clone(), "fn b() {}".into(), 0, "java".into(), None);
        manager.set_active_path(&a);
        manager.update(
            &a,
            DocumentUpdate {
                source: "fn a() { /* unsaved */ }".into(),
                saved_source: "fn a() {}".into(),
                cursor_byte: 8,
                mode: "rust".into(),
                disk_stamp: None,
                external_change: false,
            },
        );
        manager.set_active_path(&b);
        manager.update(
            &b,
            DocumentUpdate {
                source: "fn b() { /* also unsaved */ }".into(),
                saved_source: "fn b() {}".into(),
                cursor_byte: 9,
                mode: "compare".into(),
                disk_stamp: None,
                external_change: false,
            },
        );

        assert_eq!(manager.len(), 2);
        assert_eq!(manager.dirty_count(), 2);
        assert_eq!(manager.active_path(), Some(b.clone()));
        assert!(manager.get(&a).unwrap().source.contains("unsaved"));
        assert_eq!(manager.get(&a).unwrap().cursor_byte, 8);
        assert_eq!(manager.get(&b).unwrap().mode, "compare");
    }

    #[test]
    fn reactivating_a_document_returns_its_own_state_and_close_selects_neighbor() {
        let mut manager = DocumentManager::default();
        let a = PathBuf::from("/tmp/rustbridge-tab-a.rs");
        let b = PathBuf::from("/tmp/rustbridge-tab-b.rs");
        manager.insert_loaded(a.clone(), "a".into(), 0, "rust".into(), None);
        manager.insert_loaded(b.clone(), "b".into(), 0, "java".into(), None);
        manager.set_active_path(&a);
        manager.update(
            &a,
            DocumentUpdate {
                source: "edited a".into(),
                saved_source: "a".into(),
                cursor_byte: 4,
                mode: "rust".into(),
                disk_stamp: None,
                external_change: false,
            },
        );

        let restored = manager.activate(&a).unwrap();
        assert_eq!(restored.source, "edited a");
        assert!(restored.is_dirty());
        manager.remove(&a).unwrap();
        assert_eq!(manager.active_path(), Some(b));
    }

    #[test]
    fn duplicate_open_does_not_replace_an_existing_dirty_buffer() {
        let mut manager = DocumentManager::default();
        let path = PathBuf::from("/tmp/rustbridge-duplicate.rs");
        manager.insert_loaded(path.clone(), "disk".into(), 0, "rust".into(), None);
        manager.update(
            &path,
            DocumentUpdate {
                source: "unsaved".into(),
                saved_source: "disk".into(),
                cursor_byte: 2,
                mode: "rust".into(),
                disk_stamp: None,
                external_change: false,
            },
        );
        let id = manager.insert_loaded(path.clone(), "external".into(), 0, "java".into(), None);
        assert_eq!(manager.len(), 1);
        assert_eq!(manager.get(&path).unwrap().source, "unsaved");
        assert_eq!(manager.get(&path).unwrap().id, id);
    }
}
