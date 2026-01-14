use crate::entities::ActionList;
/// CRDT module for managing Automerge documents
///
/// This module provides the core CRDT functionality for ClearHead:
/// - Converting ActionList to/from Automerge documents
/// - Managing local CRDT storage (~/.clearhead/repos/)
/// - Syncing operations between devices
///
/// Architecture:
/// - CRDT stores current active state (templates + recent completions)
/// - events.db stores full history (local, append-only)
/// - .actions files are generated views of CRDT state
use automerge::AutoCommit;
use autosurgeon::{Hydrate, Reconcile, hydrate, reconcile};
use std::path::{Path, PathBuf};

/// Wrapper struct for ActionList to satisfy autosurgeon's map requirement
#[derive(Debug, Clone, Reconcile, Hydrate)]
struct ActionListWrapper {
    actions: ActionList,
}

/// Wrapper around an Automerge document containing an ActionList
#[derive(Debug)]
pub struct ActionListDoc {
    doc: AutoCommit,
    /// Path to the .actions file this CRDT represents
    file_path: PathBuf,
}

/// Storage location for CRDT documents
#[derive(Debug, Clone)]
pub struct CrdtStorage {
    /// Base directory for CRDT storage (e.g., ~/.clearhead/repos/)
    storage_dir: PathBuf,
}

impl CrdtStorage {
    /// Create a new CRDT storage manager
    ///
    /// Uses ~/.clearhead/repos/ by default
    pub fn new() -> Result<Self, String> {
        let home =
            std::env::var("HOME").map_err(|_| "Could not determine HOME directory".to_string())?;

        let storage_dir = PathBuf::from(home).join(".clearhead").join("repos");

        // Create directory if it doesn't exist
        std::fs::create_dir_all(&storage_dir)
            .map_err(|e| format!("Failed to create CRDT storage directory: {}", e))?;

        Ok(CrdtStorage { storage_dir })
    }

    /// Create a CRDT storage with a custom directory (for testing)
    pub fn with_dir(storage_dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&storage_dir)
            .map_err(|e| format!("Failed to create CRDT storage directory: {}", e))?;

        Ok(CrdtStorage { storage_dir })
    }

    /// Get the CRDT file path for a given .actions file
    ///
    /// Maps: ~/Products/platform/next.actions -> ~/.clearhead/repos/platform.automerge
    pub fn crdt_path_for_file(&self, actions_file: &Path) -> PathBuf {
        // Get the parent directory name (e.g., "platform" from "~/Products/platform")
        let dir_name = actions_file
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("default");

        self.storage_dir.join(format!("{}.automerge", dir_name))
    }

    /// Load an existing CRDT document for a file
    pub fn load(&self, actions_file: &Path) -> Result<ActionListDoc, String> {
        let crdt_path = self.crdt_path_for_file(actions_file);

        if !crdt_path.exists() {
            return Err(format!("CRDT file does not exist: {}", crdt_path.display()));
        }

        let bytes =
            std::fs::read(&crdt_path).map_err(|e| format!("Failed to read CRDT file: {}", e))?;

        let doc = AutoCommit::load(bytes.as_slice())
            .map_err(|e| format!("Failed to load AutoCommit document: {}", e))?;

        Ok(ActionListDoc {
            doc,
            file_path: actions_file.to_path_buf(),
        })
    }

    /// Save a CRDT document to disk
    pub fn save(&self, doc: &mut ActionListDoc) -> Result<(), String> {
        let crdt_path = self.crdt_path_for_file(&doc.file_path);

        let bytes = doc.doc.save();

        std::fs::write(&crdt_path, &bytes)
            .map_err(|e| format!("Failed to write CRDT file: {}", e))?;

        Ok(())
    }

    /// Check if a CRDT document exists for a file
    pub fn exists(&self, actions_file: &Path) -> bool {
        self.crdt_path_for_file(actions_file).exists()
    }
}

impl ActionListDoc {
    /// Create a new CRDT document from an ActionList
    pub fn new(file_path: PathBuf, actions: &ActionList) -> Result<Self, String> {
        let mut doc = AutoCommit::new();

        // Wrap ActionList in a struct for autosurgeon
        let wrapper = ActionListWrapper {
            actions: actions.clone(),
        };

        // Put the wrapped ActionList into the document
        reconcile(&mut doc, &wrapper)
            .map_err(|e| format!("Failed to reconcile ActionList into Automerge: {}", e))?;

        Ok(ActionListDoc { doc, file_path })
    }

    /// Extract the current ActionList from the CRDT
    pub fn to_action_list(&self) -> Result<ActionList, String> {
        let wrapper: ActionListWrapper = hydrate(&self.doc)
            .map_err(|e| format!("Failed to hydrate ActionList from Automerge: {}", e))?;
        Ok(wrapper.actions)
    }

    /// Update the CRDT with a new ActionList
    ///
    /// This performs a reconciliation, applying changes to the existing CRDT state
    pub fn update(&mut self, actions: &ActionList) -> Result<(), String> {
        let wrapper = ActionListWrapper {
            actions: actions.clone(),
        };
        reconcile(&mut self.doc, &wrapper)
            .map_err(|e| format!("Failed to reconcile ActionList update: {}", e))?;

        Ok(())
    }

    /// Get a reference to the underlying AutoCommit document
    pub fn doc(&self) -> &AutoCommit {
        &self.doc
    }

    /// Get a mutable reference to the underlying AutoCommit document
    pub fn doc_mut(&mut self) -> &mut AutoCommit {
        &mut self.doc
    }

    /// Merge changes from another document
    ///
    /// This is used for syncing between devices
    pub fn merge(&mut self, other: &mut AutoCommit) -> Result<(), String> {
        self.doc
            .merge(other)
            .map_err(|e| format!("Failed to merge Automerge documents: {}", e))?;

        Ok(())
    }

    /// Get the file path this document represents
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }
}

/// Repository pattern for managing the lifecycle of Actions
///
/// This struct encapsulates the "Hub-and-Spoke" architecture:
/// 1. Hub: The CRDT (Source of Truth)
/// 2. Spoke: The File (User View)
///
/// It handles the "Sync In -> Modify -> Sync Out" flow to ensure
/// the file and CRDT remain consistent.
pub struct ActionRepository {
    storage: CrdtStorage,
    file_path: PathBuf,
    doc: ActionListDoc,
}

impl ActionRepository {
    /// Load a repository for a specific actions file
    ///
    /// This performs the "Sync In" phase:
    /// 1. Loads the CRDT (if it exists)
    /// 2. Reads the current file content
    /// 3. Reconciles the file content into the CRDT (capturing manual edits)
    pub fn load(file_path: PathBuf) -> Result<Self, String> {
        let storage = CrdtStorage::new()?;

        // 1. Read and parse current file
        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read file '{}': {}", file_path.display(), e))?;

        // Use the parser from lib.rs
        use crate::get_action_list_struct;
        // Pass empty json options
        let file_actions = get_action_list_struct(&serde_json::json!({}), &content)?;

        // 2. Load or Create CRDT
        let mut doc = if storage.exists(&file_path) {
            storage.load(&file_path)?
        } else {
            ActionListDoc::new(file_path.clone(), &file_actions)?
        };

        // 3. Reconcile File -> CRDT (Sync In)
        // This ensures any manual edits since last run are captured
        doc.update(&file_actions)?;

        Ok(Self {
            storage,
            file_path,
            doc,
        })
    }

    /// Get the current list of actions from the Source of Truth (CRDT)
    pub fn get_actions(&self) -> Result<ActionList, String> {
        self.doc.to_action_list()
    }

    /// Update the repository with a modified action list
    ///
    /// This performs the "Sync Out" phase:
    /// 1. Reconciles the changes into the CRDT
    /// 2. Persists the CRDT to disk
    /// 3. Renders the formatted actions back to the file
    pub fn save(&mut self, actions: &ActionList) -> Result<(), String> {
        // 1. Update CRDT (Source of Truth)
        self.doc.update(actions)?;

        // 2. Persist CRDT
        self.storage.save(&mut self.doc)?;

        // 3. Render to File (View)
        use crate::{OutputFormat, format};
        let formatted = format(actions, OutputFormat::Actions, None)?;

        std::fs::write(&self.file_path, formatted).map_err(|e| {
            format!(
                "Failed to write to file '{}': {}",
                self.file_path.display(),
                e
            )
        })?;

        Ok(())
    }
}

/// Initialize CRDT for an actions file
///
/// If CRDT doesn't exist, create it from the file.
/// If it does exist, load it and check if it needs updating.
pub fn init_crdt_for_file(file_path: &Path, actions: &ActionList) -> Result<ActionListDoc, String> {
    let storage = CrdtStorage::new()?;

    if storage.exists(file_path) {
        // Load existing CRDT
        let doc = storage.load(file_path)?;

        // TODO: Check if file has changed since last CRDT update
        // For now, just return the existing CRDT
        Ok(doc)
    } else {
        // Create new CRDT from actions
        let mut doc = ActionListDoc::new(file_path.to_path_buf(), actions)?;
        storage.save(&mut doc)?;
        Ok(doc)
    }
}

/// Sync file changes to CRDT
///
/// Compares the current ActionList with the CRDT state and updates if changed
pub fn sync_file_to_crdt(file_path: &Path, actions: &ActionList) -> Result<ActionListDoc, String> {
    let storage = CrdtStorage::new()?;

    let mut doc = if storage.exists(file_path) {
        storage.load(file_path)?
    } else {
        ActionListDoc::new(file_path.to_path_buf(), actions)?
    };

    // Update CRDT with current actions
    doc.update(actions)?;

    // Save updated CRDT
    storage.save(&mut doc)?;

    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{Action, ActionState};
    use tempfile::TempDir;
    use uuid::Uuid;

    fn create_test_action(name: &str) -> Action {
        Action {
            id: Uuid::new_v4(),
            parent_id: None,
            state: ActionState::NotStarted,
            name: name.to_string(),
            description: None,
            priority: None,
            context_list: None,
            do_date_time: None,
            do_duration: None,
            recurrence: None,
            completed_date_time: None,
            created_date_time: None,
            predecessors: None,
            story: None,
        }
    }

    #[test]
    fn test_create_and_load_crdt() {
        let temp_dir = TempDir::new().unwrap();
        let storage_dir = temp_dir.path().join("crdt_storage");
        let storage = CrdtStorage::with_dir(storage_dir).unwrap();

        let actions_file = temp_dir.path().join("test.actions");
        let actions = vec![create_test_action("Task 1"), create_test_action("Task 2")];

        // Create CRDT
        let mut doc = ActionListDoc::new(actions_file.clone(), &actions).unwrap();
        storage.save(&mut doc).unwrap();

        // Load CRDT
        let loaded_doc = storage.load(&actions_file).unwrap();
        let loaded_actions = loaded_doc.to_action_list().unwrap();

        assert_eq!(loaded_actions.len(), 2);
        assert_eq!(loaded_actions[0].name, "Task 1");
        assert_eq!(loaded_actions[1].name, "Task 2");
    }

    #[test]
    fn test_update_crdt() {
        let temp_dir = TempDir::new().unwrap();
        let actions_file = temp_dir.path().join("test.actions");

        let actions = vec![create_test_action("Task 1")];

        let mut doc = ActionListDoc::new(actions_file.clone(), &actions).unwrap();

        // Update with more actions
        let updated_actions = vec![
            create_test_action("Task 1"),
            create_test_action("Task 2"),
            create_test_action("Task 3"),
        ];

        doc.update(&updated_actions).unwrap();

        let result = doc.to_action_list().unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_crdt_path_mapping() {
        let temp_dir = TempDir::new().unwrap();
        let storage = CrdtStorage::with_dir(temp_dir.path().to_path_buf()).unwrap();

        let actions_file = PathBuf::from("/home/user/Products/platform/next.actions");
        let crdt_path = storage.crdt_path_for_file(&actions_file);

        assert_eq!(
            crdt_path.file_name().unwrap().to_str().unwrap(),
            "platform.automerge"
        );
    }

    #[test]
    fn test_round_trip_with_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let actions_file = temp_dir.path().join("test.actions");

        let mut action = create_test_action("Task with metadata");
        action.priority = Some(1);
        action.description = Some("Test description".to_string());
        action.context_list = Some(vec!["work".to_string(), "urgent".to_string()]);

        let actions = vec![action.clone()];

        let doc = ActionListDoc::new(actions_file, &actions).unwrap();
        let result = doc.to_action_list().unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Task with metadata");
        assert_eq!(result[0].priority, Some(1));
        assert_eq!(result[0].description, Some("Test description".to_string()));
        assert_eq!(
            result[0].context_list,
            Some(vec!["work".to_string(), "urgent".to_string()])
        );
    }
}
