use crate::entities::ActionList;
/// CRDT module for managing Automerge documents
///
/// This module provides the core CRDT functionality for ClearHead:
/// - Converting ActionList to/from Automerge documents
/// - Managing local CRDT storage per XDG spec
/// - Syncing operations between devices
///
/// Architecture (CRDT-first):
/// - CRDT is the source of truth (stored in XDG_STATE_HOME)
/// - DSL files are projections of CRDT state
/// - events.db stores full history for analytics (see event_logging_specification)
///
/// Storage locations:
/// - Global workspace: $XDG_STATE_HOME/clearhead/workspace.crdt
/// - Project workspace: <project>/.clearhead/workspace.crdt
/// - Shadow files: /tmp/clearhead-shadow/ (for 3-way merge)
///
/// See: specifications/sync_architecture.md
use automerge::AutoCommit;
use autosurgeon::{Hydrate, Reconcile, hydrate, reconcile};
use std::path::{Path, PathBuf};

/// Name of the CRDT file within a workspace
const CRDT_FILENAME: &str = "workspace.crdt";

/// Directory for shadow files (used in 3-way merge)
const SHADOW_DIR: &str = "/tmp/clearhead-shadow";

/// Wrapper struct for ActionList to satisfy autosurgeon's map requirement
#[derive(Debug, Clone, Reconcile, Hydrate)]
struct ActionListWrapper {
    actions: ActionList,
}

/// Wrapper around an Automerge document containing an ActionList
#[derive(Debug)]
pub struct ActionListDoc {
    doc: AutoCommit,
    /// The workspace this document belongs to
    workspace: Workspace,
}

/// Represents a workspace (global or project-local)
#[derive(Debug, Clone)]
pub enum Workspace {
    /// Global workspace at XDG_STATE_HOME/clearhead/
    Global { state_dir: PathBuf },
    /// Project-local workspace at <project>/.clearhead/
    Project { project_root: PathBuf },
}

impl Workspace {
    /// Detect the workspace for a given file path
    ///
    /// Searches upward from the file's directory for:
    /// 1. .clearhead/ directory (project workspace)
    /// 2. next.actions file (lightweight project marker)
    ///
    /// Falls back to global workspace if no project found.
    pub fn detect(file_path: &Path) -> Result<Self, String> {
        let canonical = file_path
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize path: {}", e))?;

        let start_dir = if canonical.is_file() {
            canonical.parent().unwrap_or(&canonical)
        } else {
            &canonical
        };

        // Search upward for project markers
        let mut current = start_dir.to_path_buf();
        loop {
            // Check for .clearhead/ directory
            let clearhead_dir = current.join(".clearhead");
            if clearhead_dir.is_dir() {
                return Ok(Workspace::Project { project_root: current });
            }

            // Check for next.actions file (lightweight project marker)
            let next_actions = current.join("next.actions");
            if next_actions.is_file() && file_path != next_actions {
                // Found project root, but no .clearhead/ - use global workspace
                // but associate with this project
                return Ok(Workspace::Project { project_root: current });
            }

            // Move up one directory
            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => break, // Reached filesystem root
            }
        }

        // No project found, use global workspace
        Ok(Workspace::global()?)
    }

    /// Create a global workspace using XDG_STATE_HOME
    pub fn global() -> Result<Self, String> {
        let state_dir = get_xdg_state_home()?;
        Ok(Workspace::Global { state_dir })
    }

    /// Get the path to the CRDT file for this workspace
    pub fn crdt_path(&self) -> PathBuf {
        match self {
            Workspace::Global { state_dir } => state_dir.join(CRDT_FILENAME),
            Workspace::Project { project_root } => {
                project_root.join(".clearhead").join(CRDT_FILENAME)
            }
        }
    }

    /// Get the directory containing the CRDT (for creating if needed)
    pub fn crdt_dir(&self) -> PathBuf {
        match self {
            Workspace::Global { state_dir } => state_dir.clone(),
            Workspace::Project { project_root } => project_root.join(".clearhead"),
        }
    }

    /// Ensure the workspace directory exists
    pub fn ensure_dir(&self) -> Result<(), String> {
        let dir = self.crdt_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create workspace directory '{}': {}", dir.display(), e))
    }
}

/// Get XDG_STATE_HOME/clearhead, creating if needed
fn get_xdg_state_home() -> Result<PathBuf, String> {
    let state_home = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".local").join("state")
        });

    let clearhead_state = state_home.join("clearhead");
    std::fs::create_dir_all(&clearhead_state)
        .map_err(|e| format!("Failed to create state directory: {}", e))?;

    Ok(clearhead_state)
}

/// Storage manager for CRDT documents
///
/// Handles loading/saving CRDT files and manages workspace detection.
#[derive(Debug, Clone)]
pub struct CrdtStorage {
    workspace: Workspace,
}

impl CrdtStorage {
    /// Create storage for a specific workspace
    pub fn new(workspace: Workspace) -> Result<Self, String> {
        workspace.ensure_dir()?;
        Ok(CrdtStorage { workspace })
    }

    /// Create storage by detecting workspace from a file path
    pub fn for_file(file_path: &Path) -> Result<Self, String> {
        let workspace = Workspace::detect(file_path)?;
        Self::new(workspace)
    }

    /// Create storage for the global workspace
    pub fn global() -> Result<Self, String> {
        let workspace = Workspace::global()?;
        Self::new(workspace)
    }

    /// Create a CRDT storage with a custom directory (for testing)
    pub fn with_dir(storage_dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&storage_dir)
            .map_err(|e| format!("Failed to create CRDT storage directory: {}", e))?;

        Ok(CrdtStorage {
            workspace: Workspace::Global { state_dir: storage_dir },
        })
    }

    /// Get the CRDT file path
    pub fn crdt_path(&self) -> PathBuf {
        self.workspace.crdt_path()
    }

    /// Load the CRDT document
    pub fn load(&self) -> Result<ActionListDoc, String> {
        let crdt_path = self.crdt_path();

        if !crdt_path.exists() {
            return Err(format!("CRDT file does not exist: {}", crdt_path.display()));
        }

        let bytes =
            std::fs::read(&crdt_path).map_err(|e| format!("Failed to read CRDT file: {}", e))?;

        let doc = AutoCommit::load(bytes.as_slice())
            .map_err(|e| format!("Failed to load AutoCommit document: {}", e))?;

        Ok(ActionListDoc {
            doc,
            workspace: self.workspace.clone(),
        })
    }

    /// Save a CRDT document to disk
    pub fn save(&self, doc: &mut ActionListDoc) -> Result<(), String> {
        let crdt_path = self.crdt_path();

        // Ensure directory exists
        if let Some(parent) = crdt_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create CRDT directory: {}", e))?;
        }

        let bytes = doc.doc.save();

        std::fs::write(&crdt_path, &bytes)
            .map_err(|e| format!("Failed to write CRDT file: {}", e))?;

        Ok(())
    }

    /// Check if a CRDT document exists
    pub fn exists(&self) -> bool {
        self.crdt_path().exists()
    }

    /// Get a reference to the workspace
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }
}

impl ActionListDoc {
    /// Create a new CRDT document from an ActionList
    pub fn new(workspace: Workspace, actions: &ActionList) -> Result<Self, String> {
        let mut doc = AutoCommit::new();

        // Wrap ActionList in a struct for autosurgeon
        let wrapper = ActionListWrapper {
            actions: actions.clone(),
        };

        // Put the wrapped ActionList into the document
        reconcile(&mut doc, &wrapper)
            .map_err(|e| format!("Failed to reconcile ActionList into Automerge: {}", e))?;

        Ok(ActionListDoc { doc, workspace })
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

    /// Get a reference to the workspace
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }
}

/// Repository pattern for managing the lifecycle of Actions
///
/// This struct encapsulates the CRDT-first architecture:
/// 1. CRDT is the source of truth
/// 2. Files are projections of CRDT state
///
/// It handles:
/// - Loading CRDT state
/// - Reconciling file edits back into CRDT
/// - Projecting CRDT to files (with shadow for 3-way merge)
pub struct ActionRepository {
    storage: CrdtStorage,
    file_path: PathBuf,
    doc: ActionListDoc,
}

impl ActionRepository {
    /// Load a repository for a specific actions file
    ///
    /// CRDT-first flow:
    /// 1. Detect workspace for the file
    /// 2. Load CRDT if it exists, or create from file
    /// 3. If file exists, reconcile any edits into CRDT
    pub fn load(file_path: PathBuf) -> Result<Self, String> {
        let storage = CrdtStorage::for_file(&file_path)?;

        // Check if CRDT exists
        let mut doc = if storage.exists() {
            // CRDT exists - load it
            storage.load()?
        } else {
            // No CRDT - check if file exists to bootstrap from
            if file_path.exists() {
                let content = std::fs::read_to_string(&file_path)
                    .map_err(|e| format!("Failed to read file '{}': {}", file_path.display(), e))?;

                use crate::get_action_list_struct;
                let file_actions = get_action_list_struct(&serde_json::json!({}), &content)?;

                ActionListDoc::new(storage.workspace().clone(), &file_actions)?
            } else {
                // No file either - create empty CRDT
                ActionListDoc::new(storage.workspace().clone(), &vec![])?
            }
        };

        // If file exists, reconcile any edits since last CRDT save
        if file_path.exists() {
            let content = std::fs::read_to_string(&file_path)
                .map_err(|e| format!("Failed to read file '{}': {}", file_path.display(), e))?;

            use crate::get_action_list_struct;
            let file_actions = get_action_list_struct(&serde_json::json!({}), &content)?;

            // Reconcile file -> CRDT
            doc.update(&file_actions)?;
        }

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
    /// This performs:
    /// 1. Reconcile changes into CRDT
    /// 2. Persist CRDT to disk
    /// 3. Project CRDT to file (with shadow for merge)
    pub fn save(&mut self, actions: &ActionList) -> Result<(), String> {
        // 1. Update CRDT (Source of Truth)
        self.doc.update(actions)?;

        // 2. Persist CRDT
        self.storage.save(&mut self.doc)?;

        // 3. Project to File (with shadow)
        self.project_to_file(actions)?;

        Ok(())
    }

    /// Project CRDT state to file
    ///
    /// Creates shadow copy in /tmp for 3-way merge before writing
    fn project_to_file(&self, actions: &ActionList) -> Result<(), String> {
        // Write shadow copy for 3-way merge (if file exists)
        if self.file_path.exists() {
            write_shadow_file(&self.file_path)?;
        }

        // Format and write file
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

    /// Get the file path this repository manages
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// Get the workspace this repository belongs to
    pub fn workspace(&self) -> &Workspace {
        self.storage.workspace()
    }
}

/// Write a shadow copy of a file for 3-way merge
///
/// Copies file_path to /tmp/clearhead-shadow/<filename>.base
fn write_shadow_file(file_path: &Path) -> Result<(), String> {
    let shadow_dir = PathBuf::from(SHADOW_DIR);
    std::fs::create_dir_all(&shadow_dir)
        .map_err(|e| format!("Failed to create shadow directory: {}", e))?;

    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let shadow_path = shadow_dir.join(format!("{}.base", filename));

    std::fs::copy(file_path, &shadow_path)
        .map_err(|e| format!("Failed to write shadow file: {}", e))?;

    Ok(())
}

/// Initialize CRDT for an actions file
///
/// If CRDT doesn't exist, create it from the provided actions.
/// If it does exist, load and return it.
pub fn init_crdt_for_file(file_path: &Path, actions: &ActionList) -> Result<ActionListDoc, String> {
    let storage = CrdtStorage::for_file(file_path)?;

    if storage.exists() {
        // Load existing CRDT
        storage.load()
    } else {
        // Create new CRDT from actions
        let mut doc = ActionListDoc::new(storage.workspace().clone(), actions)?;
        storage.save(&mut doc)?;
        Ok(doc)
    }
}

/// Sync file changes to CRDT
///
/// Parses the file and reconciles changes into the CRDT
pub fn sync_file_to_crdt(file_path: &Path, actions: &ActionList) -> Result<ActionListDoc, String> {
    let storage = CrdtStorage::for_file(file_path)?;

    let mut doc = if storage.exists() {
        storage.load()?
    } else {
        ActionListDoc::new(storage.workspace().clone(), actions)?
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
    use crate::entities::Action;
    use tempfile::TempDir;

    fn create_test_action(name: &str) -> Action {
        Action::new(name)
    }

    fn create_test_workspace(temp_dir: &TempDir) -> Workspace {
        Workspace::Global {
            state_dir: temp_dir.path().to_path_buf(),
        }
    }

    #[test]
    fn test_create_and_load_crdt() {
        let temp_dir = TempDir::new().unwrap();
        let storage = CrdtStorage::with_dir(temp_dir.path().to_path_buf()).unwrap();
        let workspace = create_test_workspace(&temp_dir);

        let actions = vec![create_test_action("Task 1"), create_test_action("Task 2")];

        // Create CRDT
        let mut doc = ActionListDoc::new(workspace, &actions).unwrap();
        storage.save(&mut doc).unwrap();

        // Load CRDT
        let loaded_doc = storage.load().unwrap();
        let loaded_actions = loaded_doc.to_action_list().unwrap();

        assert_eq!(loaded_actions.len(), 2);
        assert_eq!(loaded_actions[0].name, "Task 1");
        assert_eq!(loaded_actions[1].name, "Task 2");
    }

    #[test]
    fn test_update_crdt() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = create_test_workspace(&temp_dir);

        let actions = vec![create_test_action("Task 1")];

        let mut doc = ActionListDoc::new(workspace, &actions).unwrap();

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
    fn test_workspace_crdt_path() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = Workspace::Global {
            state_dir: temp_dir.path().to_path_buf(),
        };

        let crdt_path = workspace.crdt_path();

        assert_eq!(
            crdt_path.file_name().unwrap().to_str().unwrap(),
            "workspace.crdt"
        );
    }

    #[test]
    fn test_project_workspace_crdt_path() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = Workspace::Project {
            project_root: temp_dir.path().to_path_buf(),
        };

        let crdt_path = workspace.crdt_path();

        assert!(crdt_path.to_str().unwrap().contains(".clearhead"));
        assert_eq!(
            crdt_path.file_name().unwrap().to_str().unwrap(),
            "workspace.crdt"
        );
    }

    #[test]
    fn test_round_trip_with_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = create_test_workspace(&temp_dir);

        let mut action = create_test_action("Task with metadata");
        action.priority = Some(1);
        action.description = Some("Test description".to_string());
        action.context_list = Some(vec!["work".to_string(), "urgent".to_string()]);

        let actions = vec![action.clone()];

        let doc = ActionListDoc::new(workspace, &actions).unwrap();
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
