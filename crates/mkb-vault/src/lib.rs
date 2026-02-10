//! # mkb-vault
//!
//! File system operations and CRUD for the MKB vault.
//!
//! The vault is the authoritative source of truth. All knowledge
//! lives as markdown files in the vault directory. The index layer
//! is a derived cache that can be rebuilt from vault files.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use mkb_core::document::Document;
use mkb_core::error::MkbError;
use mkb_core::frontmatter::{parse_document, write_document};
use mkb_core::temporal::TemporalGate;

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

    /// Return the rejected directory path.
    #[must_use]
    pub fn rejected_dir(&self) -> PathBuf {
        self.root.join(".mkb").join("ingestion").join("rejected")
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
}
