//! # mkb-vault
//!
//! File system operations and CRUD for the MKB vault.
//!
//! The vault is the authoritative source of truth. All knowledge
//! lives as markdown files in the vault directory. The index layer
//! is a derived cache that can be rebuilt from vault files.

pub mod watcher;

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use mkb_core::document::Document;
use mkb_core::error::MkbError;
use mkb_core::frontmatter::{parse_document, write_document};
use mkb_core::temporal::TemporalGate;
use mkb_core::view::SavedView;

/// Standard vault directory structure.
const ARCHIVE_DIR: &str = ".archive";
/// The Vault manages file-system storage of knowledge documents.
#[derive(Debug)]
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    /// Open an existing vault at the given root directory.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Vault`] if the directory does not exist or
    /// is not a valid vault.
    pub fn open(root: &Path) -> Result<Self, MkbError> {
        let mkb_dir = root.join(".mkb");
        if !mkb_dir.exists() {
            return Err(MkbError::Vault(format!(
                "Not an MKB vault: {} (missing .mkb directory). Run `mkb init` first.",
                root.display()
            )));
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// Initialize a new vault at the given root directory.
    ///
    /// Creates the `.mkb/` directory structure.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Io`] if directory creation fails.
    pub fn init(root: &Path) -> Result<Self, MkbError> {
        let mkb_dir = root.join(".mkb");
        fs::create_dir_all(&mkb_dir)?;
        fs::create_dir_all(mkb_dir.join("index"))?;
        fs::create_dir_all(mkb_dir.join("ingestion"))?;
        fs::create_dir_all(mkb_dir.join("ingestion").join("rejected"))?;
        fs::create_dir_all(mkb_dir.join("views"))?;
        fs::create_dir_all(root.join(ARCHIVE_DIR))?;

        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// Return the vault root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve the file path for a document based on its type and id.
    #[must_use]
    pub fn document_path(&self, doc_type: &str, id: &str) -> PathBuf {
        let type_dir = type_to_directory(doc_type);
        self.root.join(type_dir).join(format!("{id}.md"))
    }

    /// Create a new document in the vault.
    ///
    /// Enforces the temporal gate: documents without `observed_at` are rejected.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Temporal`] if temporal validation fails.
    /// Returns [`MkbError::Vault`] if a document with the same ID already exists.
    /// Returns [`MkbError::Io`] if file writing fails.
    pub fn create(&self, doc: &Document) -> Result<PathBuf, MkbError> {
        // Validate temporal fields (re-validate even though Document::new does it)
        TemporalGate::validate_fields(&doc.temporal)?;

        let path = self.document_path(&doc.doc_type, &doc.id);

        if path.exists() {
            return Err(MkbError::Vault(format!(
                "Document already exists: {}",
                path.display()
            )));
        }

        // Ensure the type directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = write_document(doc)?;
        fs::write(&path, content)?;

        Ok(path)
    }

    /// Read a document from the vault by type and ID.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Vault`] if the document does not exist.
    /// Returns [`MkbError::Io`] if file reading fails.
    /// Returns [`MkbError::Parse`] or [`MkbError::Serialization`] if parsing fails.
    pub fn read(&self, doc_type: &str, id: &str) -> Result<Document, MkbError> {
        let path = self.document_path(doc_type, id);

        if !path.exists() {
            return Err(MkbError::Vault(format!(
                "Document not found: {}",
                path.display()
            )));
        }

        let content = fs::read_to_string(&path)?;
        parse_document(&content)
    }

    /// Update an existing document in the vault.
    ///
    /// Preserves `_created_at`, bumps `_modified_at` to now.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Vault`] if the document does not exist.
    /// Returns [`MkbError::Temporal`] if temporal validation fails.
    pub fn update(&self, doc: &mut Document) -> Result<PathBuf, MkbError> {
        let path = self.document_path(&doc.doc_type, &doc.id);

        if !path.exists() {
            return Err(MkbError::Vault(format!(
                "Document not found for update: {}",
                path.display()
            )));
        }

        // Validate temporal fields
        TemporalGate::validate_fields(&doc.temporal)?;

        // Read existing to preserve created_at
        let existing = self.read(&doc.doc_type, &doc.id)?;
        doc.created_at = existing.created_at;
        doc.modified_at = Utc::now();

        let content = write_document(doc)?;
        fs::write(&path, content)?;

        Ok(path)
    }

    /// Soft-delete a document by moving it to the archive directory.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Vault`] if the document does not exist.
    /// Returns [`MkbError::Io`] if the move fails.
    pub fn delete(&self, doc_type: &str, id: &str) -> Result<PathBuf, MkbError> {
        let path = self.document_path(doc_type, id);

        if !path.exists() {
            return Err(MkbError::Vault(format!(
                "Document not found for deletion: {}",
                path.display()
            )));
        }

        let archive_type_dir = self
            .root
            .join(ARCHIVE_DIR)
            .join(type_to_directory(doc_type));
        fs::create_dir_all(&archive_type_dir)?;

        let archive_path = archive_type_dir.join(format!("{id}.md"));
        fs::rename(&path, &archive_path)?;

        Ok(archive_path)
    }

    /// List all document files in the vault (recursively scans type directories).
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Io`] if directory reading fails.
    pub fn list_documents(&self) -> Result<Vec<PathBuf>, MkbError> {
        let mut docs = Vec::new();
        self.scan_directory(&self.root, &mut docs)?;
        Ok(docs)
    }

    // === Saved Views ===

    /// Return the views directory path.
    #[must_use]
    pub fn views_dir(&self) -> PathBuf {
        self.root.join(".mkb").join("views")
    }

    /// Save a named view (MKQL query) to `.mkb/views/{name}.yaml`.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Io`] if writing fails.
    pub fn save_view(&self, view: &SavedView) -> Result<PathBuf, MkbError> {
        let dir = self.views_dir();
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.yaml", view.name));
        let yaml =
            serde_yaml::to_string(view).map_err(|e| MkbError::Serialization(e.to_string()))?;
        fs::write(&path, yaml)?;
        Ok(path)
    }

    /// Load a saved view by name.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Vault`] if the view does not exist.
    /// Returns [`MkbError::Serialization`] if parsing fails.
    pub fn load_view(&self, name: &str) -> Result<SavedView, MkbError> {
        let path = self.views_dir().join(format!("{name}.yaml"));
        if !path.exists() {
            return Err(MkbError::Vault(format!("View not found: {name}")));
        }
        let content = fs::read_to_string(&path)?;
        serde_yaml::from_str(&content).map_err(|e| MkbError::Serialization(e.to_string()))
    }

    /// List all saved views (returns view names without extension).
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Io`] if directory reading fails.
    pub fn list_views(&self) -> Result<Vec<String>, MkbError> {
        let dir = self.views_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    /// Delete a saved view by name.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Vault`] if the view does not exist.
    /// Returns [`MkbError::Io`] if deletion fails.
    pub fn delete_view(&self, name: &str) -> Result<(), MkbError> {
        let path = self.views_dir().join(format!("{name}.yaml"));
        if !path.exists() {
            return Err(MkbError::Vault(format!("View not found: {name}")));
        }
        fs::remove_file(&path)?;
        Ok(())
    }

    /// Return the rejected directory path.
    #[must_use]
    pub fn rejected_dir(&self) -> PathBuf {
        self.root.join(".mkb").join("ingestion").join("rejected")
    }

    /// Write a rejected document to the rejection log.
    ///
    /// Stores the raw content and error details in `.mkb/ingestion/rejected/`.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Io`] if file writing fails.
    pub fn write_rejection(
        &self,
        filename: &str,
        raw_content: &str,
        error: &str,
        extraction_attempts: &[String],
    ) -> Result<PathBuf, MkbError> {
        let rejected_dir = self.rejected_dir();
        fs::create_dir_all(&rejected_dir)?;

        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let reject_filename = format!("{timestamp}-{filename}");
        let path = rejected_dir.join(&reject_filename);

        let mut content = String::new();
        content.push_str("---\n");
        content.push_str(&format!("rejected_at: \"{}\"\n", Utc::now().to_rfc3339()));
        content.push_str(&format!("error: \"{error}\"\n"));
        content.push_str(&format!("original_file: \"{filename}\"\n"));
        if !extraction_attempts.is_empty() {
            content.push_str("extraction_attempts:\n");
            for attempt in extraction_attempts {
                content.push_str(&format!("  - \"{attempt}\"\n"));
            }
        }
        content.push_str("---\n\n");
        content.push_str(raw_content);

        fs::write(&path, content)?;
        Ok(path)
    }

    /// Count rejected documents in the rejection log.
    ///
    /// # Errors
    ///
    /// Returns [`MkbError::Io`] if directory reading fails.
    pub fn rejection_count(&self) -> Result<usize, MkbError> {
        let rejected_dir = self.rejected_dir();
        if !rejected_dir.exists() {
            return Ok(0);
        }
        let count = fs::read_dir(&rejected_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|ext| ext.to_str()).is_some())
            .count();
        Ok(count)
    }

    fn scan_directory(&self, dir: &Path, docs: &mut Vec<PathBuf>) -> Result<(), MkbError> {
        if !dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            // Skip hidden directories (.mkb, .archive, etc.)
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with('.') {
                    self.scan_directory(&path, docs)?;
                }
                continue;
            }

            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                docs.push(path);
            }
        }

        Ok(())
    }
}

/// Find the next available counter for a document ID to avoid collisions.
///
/// Scans the type directory for existing files matching the pattern
/// and returns the next counter value.
#[must_use]
pub fn next_counter(vault_root: &Path, doc_type: &str, slug: &str) -> u32 {
    let type_dir = vault_root.join(type_to_directory(doc_type));
    let type_prefix = &doc_type[..doc_type.len().min(4)];
    let pattern = format!("{type_prefix}-{slug}-");

    if !type_dir.exists() {
        return 1;
    }

    let mut max_counter: u32 = 0;
    if let Ok(entries) = fs::read_dir(&type_dir) {
        for entry in entries.flatten() {
            let name = entry
                .path()
                .file_stem()
                .and_then(|s| s.to_str().map(String::from))
                .unwrap_or_default();
            if name.starts_with(&pattern) {
                if let Some(counter_str) = name.strip_prefix(&pattern) {
                    if let Ok(counter) = counter_str.parse::<u32>() {
                        max_counter = max_counter.max(counter);
                    }
                }
            }
        }
    }

    max_counter + 1
}

/// Map a document type to its subdirectory name.
#[must_use]
pub fn type_to_directory(doc_type: &str) -> String {
    match doc_type {
        "project" => "projects".to_string(),
        "meeting" => "meetings".to_string(),
        "person" => "people".to_string(),
        "decision" => "decisions".to_string(),
        "signal" => "signals".to_string(),
        other => format!("{other}s"),
    }
}

/// Generate a slug from a title.
#[must_use]
pub fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use mkb_core::document::Document;
    use mkb_core::temporal::{DecayProfile, RawTemporalInput, TemporalPrecision};

    fn utc(y: i32, m: u32, d: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
    }

    fn make_doc(id: &str, doc_type: &str, title: &str) -> Document {
        let input = RawTemporalInput {
            observed_at: Some(utc(2025, 2, 10)),
            valid_until: Some(utc(2025, 8, 10)),
            temporal_precision: Some(TemporalPrecision::Day),
            occurred_at: None,
        };
        let profile = DecayProfile::default_profile();
        let mut doc = Document::new(
            id.to_string(),
            doc_type.to_string(),
            title.to_string(),
            input,
            &profile,
        )
        .unwrap();
        doc.body = format!("## {title}\n\nContent for {id}.\n");
        doc
    }

    #[test]
    fn init_creates_directory_structure() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        assert!(dir.path().join(".mkb").exists());
        assert!(dir.path().join(".mkb/index").exists());
        assert!(dir.path().join(".mkb/ingestion").exists());
        assert!(dir.path().join(".mkb/ingestion/rejected").exists());
        assert!(dir.path().join(ARCHIVE_DIR).exists());
        assert_eq!(vault.root(), dir.path());
    }

    #[test]
    fn open_fails_without_init() {
        let dir = tempfile::tempdir().unwrap();
        let result = Vault::open(dir.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("mkb init"));
    }

    #[test]
    fn create_document_writes_markdown_file() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let doc = make_doc("proj-alpha-001", "project", "Alpha Project");
        let path = vault.create(&doc).unwrap();

        assert!(path.exists());
        assert!(path.to_string_lossy().contains("projects/"));

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("proj-alpha-001"));
        assert!(content.contains("observed_at"));
        assert!(content.contains("Alpha Project"));
    }

    #[test]
    fn create_rejects_document_without_observed_at() {
        let dir = tempfile::tempdir().unwrap();
        let _vault = Vault::init(dir.path()).unwrap();

        // Document::new already enforces this via TemporalGate
        let input = RawTemporalInput::default();
        let profile = DecayProfile::default_profile();
        let result = Document::new(
            "test-001".to_string(),
            "project".to_string(),
            "Test".to_string(),
            input,
            &profile,
        );
        assert!(result.is_err());
    }

    #[test]
    fn create_rejects_duplicate_document() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let doc = make_doc("proj-alpha-001", "project", "Alpha");
        vault.create(&doc).unwrap();

        let result = vault.create(&doc);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn read_document_parses_frontmatter_and_body() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let doc = make_doc("proj-alpha-001", "project", "Alpha Project");
        vault.create(&doc).unwrap();

        let loaded = vault.read("project", "proj-alpha-001").unwrap();
        assert_eq!(loaded.id, "proj-alpha-001");
        assert_eq!(loaded.doc_type, "project");
        assert_eq!(loaded.title, "Alpha Project");
        assert_eq!(loaded.temporal.observed_at, utc(2025, 2, 10));
        assert!(loaded.body.contains("Alpha Project"));
    }

    #[test]
    fn update_preserves_created_at_bumps_modified_at() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let doc = make_doc("proj-alpha-001", "project", "Alpha");
        vault.create(&doc).unwrap();
        let original_created = doc.created_at;

        // Small sleep to ensure time difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        let mut updated = vault.read("project", "proj-alpha-001").unwrap();
        updated.title = "Alpha Updated".to_string();
        vault.update(&mut updated).unwrap();

        let reloaded = vault.read("project", "proj-alpha-001").unwrap();
        assert_eq!(reloaded.created_at, original_created);
        assert_eq!(reloaded.title, "Alpha Updated");
        // modified_at should be newer (or equal on fast machines)
        assert!(reloaded.modified_at >= original_created);
    }

    #[test]
    fn delete_soft_moves_to_archive() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let doc = make_doc("proj-alpha-001", "project", "Alpha");
        let original_path = vault.create(&doc).unwrap();
        assert!(original_path.exists());

        let archive_path = vault.delete("project", "proj-alpha-001").unwrap();
        assert!(!original_path.exists(), "Original file should be gone");
        assert!(archive_path.exists(), "Archive file should exist");
        assert!(
            archive_path.to_string_lossy().contains(ARCHIVE_DIR),
            "Should be in archive directory"
        );
    }

    #[test]
    fn list_documents_finds_all_markdown_files() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        vault
            .create(&make_doc("proj-a-001", "project", "A"))
            .unwrap();
        vault
            .create(&make_doc("meet-b-001", "meeting", "B"))
            .unwrap();

        let docs = vault.list_documents().unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn type_to_directory_maps_correctly() {
        assert_eq!(type_to_directory("project"), "projects");
        assert_eq!(type_to_directory("meeting"), "meetings");
        assert_eq!(type_to_directory("person"), "people");
        assert_eq!(type_to_directory("decision"), "decisions");
        assert_eq!(type_to_directory("signal"), "signals");
        assert_eq!(type_to_directory("note"), "notes");
    }

    #[test]
    fn slugify_generates_clean_slugs() {
        assert_eq!(slugify("Alpha Project"), "alpha-project");
        assert_eq!(slugify("Sprint Review Q4 2025"), "sprint-review-q4-2025");
        assert_eq!(slugify("hello---world"), "hello-world");
    }

    #[test]
    fn document_path_resolves_correctly() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let path = vault.document_path("project", "proj-alpha-001");
        assert!(path.to_string_lossy().contains("projects"));
        assert!(path.to_string_lossy().contains("proj-alpha-001.md"));
    }

    // === Saved Views tests ===

    #[test]
    fn vault_save_and_load_view() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let view = mkb_core::view::SavedView {
            name: "active-projects".to_string(),
            description: Some("Currently active projects".to_string()),
            query: "SELECT * FROM project WHERE CURRENT()".to_string(),
            created_at: "2025-02-10T00:00:00Z".to_string(),
        };

        let path = vault.save_view(&view).unwrap();
        assert!(path.exists());
        assert!(path
            .to_string_lossy()
            .contains("views/active-projects.yaml"));

        let loaded = vault.load_view("active-projects").unwrap();
        assert_eq!(loaded.name, "active-projects");
        assert_eq!(loaded.query, "SELECT * FROM project WHERE CURRENT()");
        assert_eq!(
            loaded.description,
            Some("Currently active projects".to_string())
        );
    }

    #[test]
    fn vault_list_views() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        // Empty initially
        assert!(vault.list_views().unwrap().is_empty());

        let view1 = mkb_core::view::SavedView {
            name: "alpha".to_string(),
            description: None,
            query: "SELECT * FROM project".to_string(),
            created_at: "2025-02-10T00:00:00Z".to_string(),
        };
        let view2 = mkb_core::view::SavedView {
            name: "beta".to_string(),
            description: None,
            query: "SELECT * FROM meeting".to_string(),
            created_at: "2025-02-10T00:00:00Z".to_string(),
        };

        vault.save_view(&view1).unwrap();
        vault.save_view(&view2).unwrap();

        let names = vault.list_views().unwrap();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn vault_delete_view() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let view = mkb_core::view::SavedView {
            name: "to-delete".to_string(),
            description: None,
            query: "SELECT * FROM project".to_string(),
            created_at: "2025-02-10T00:00:00Z".to_string(),
        };

        vault.save_view(&view).unwrap();
        assert!(vault.load_view("to-delete").is_ok());

        vault.delete_view("to-delete").unwrap();
        assert!(vault.load_view("to-delete").is_err());
    }

    #[test]
    fn vault_load_nonexistent_view_fails() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let result = vault.load_view("does-not-exist");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn vault_delete_nonexistent_view_fails() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let result = vault.delete_view("nope");
        assert!(result.is_err());
    }

    // === T-110.5 tests: rejection log ===

    #[test]
    fn rejected_doc_written_to_rejected_dir() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let path = vault
            .write_rejection(
                "bad-doc.md",
                "# Some content without frontmatter",
                "REJECTED: No temporal grounding",
                &[],
            )
            .unwrap();

        assert!(path.exists());
        assert!(path.to_string_lossy().contains("rejected"));
        assert!(path.to_string_lossy().contains("bad-doc.md"));

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("rejected_at"));
        assert!(content.contains("No temporal grounding"));
        assert!(content.contains("# Some content without frontmatter"));
    }

    #[test]
    fn rejection_includes_extraction_attempts() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        let attempts = vec![
            "date_extraction: no dates found".to_string(),
            "header_parsing: no temporal markers".to_string(),
        ];

        let path = vault
            .write_rejection(
                "undated.md",
                "# Meeting notes\nSome content here.",
                "REJECTED: No temporal grounding",
                &attempts,
            )
            .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("extraction_attempts"));
        assert!(content.contains("date_extraction"));
        assert!(content.contains("header_parsing"));
    }

    #[test]
    fn rejection_count_tracks_rejected_docs() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        assert_eq!(vault.rejection_count().unwrap(), 0);

        vault
            .write_rejection("bad1.md", "content1", "error1", &[])
            .unwrap();
        assert_eq!(vault.rejection_count().unwrap(), 1);

        vault
            .write_rejection("bad2.md", "content2", "error2", &[])
            .unwrap();
        assert_eq!(vault.rejection_count().unwrap(), 2);
    }

    // === T-110.6 tests: file path resolution ===

    #[test]
    fn collision_appends_counter() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();

        // Create first document
        let doc1 = make_doc("proj-alpha-project-001", "project", "Alpha Project");
        vault.create(&doc1).unwrap();

        // next_counter should return 2 since 001 already exists
        let counter = next_counter(dir.path(), "project", "alpha-project");
        assert_eq!(counter, 2);

        // Create second document with the next counter
        let id2 = Document::generate_id("project", "Alpha Project", counter);
        assert_eq!(id2, "proj-alpha-project-002");

        let doc2 = make_doc(&id2, "project", "Alpha Project v2");
        vault.create(&doc2).unwrap();

        // next_counter should now return 3
        let counter = next_counter(dir.path(), "project", "alpha-project");
        assert_eq!(counter, 3);
    }
}
