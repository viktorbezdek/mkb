//! End-to-end tests for the MKB CLI.
//!
//! Tests invoke the `mkb` binary as a subprocess and verify JSON output.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

fn mkb() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mkb"))
}

fn mkb_in(dir: &Path) -> Command {
    let mut cmd = mkb();
    cmd.current_dir(dir);
    cmd
}

fn init_vault() -> TempDir {
    let dir = TempDir::new().unwrap();
    let output = mkb_in(dir.path()).arg("init").arg(".").output().unwrap();
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir
}

fn add_project(dir: &Path, title: &str) -> serde_json::Value {
    let output = mkb_in(dir)
        .args([
            "add",
            "--doc-type",
            "project",
            "--title",
            title,
            "--observed-at",
            "2025-02-10T00:00:00Z",
            "--body",
            &format!("Body of {title}"),
            "--tags",
            "rust,test",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

// === T-300.1: Init ===

#[test]
fn e2e_init_creates_vault_structure() {
    let dir = TempDir::new().unwrap();
    let output = mkb_in(dir.path()).arg("init").arg(".").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Initialized MKB vault"));

    // Verify structure
    assert!(dir.path().join(".mkb").exists());
    assert!(dir
        .path()
        .join(".mkb")
        .join("index")
        .join("mkb.db")
        .exists());
}

// === T-300.2: Add ===

#[test]
fn e2e_add_creates_document_with_temporal_gate() {
    let dir = init_vault();
    let result = add_project(dir.path(), "Alpha Project");

    assert_eq!(result["type"], "project");
    assert_eq!(result["title"], "Alpha Project");
    assert!(result["id"].as_str().unwrap().starts_with("proj-"));
    assert!(result["observed_at"].as_str().is_some());
}

#[test]
fn e2e_add_rejects_without_observed_at() {
    let dir = init_vault();
    let output = mkb_in(dir.path())
        .args(["add", "--doc-type", "project", "--title", "Test"])
        .output()
        .unwrap();
    // Should fail because --observed-at is required
    assert!(!output.status.success());
}

#[test]
fn e2e_add_from_file() {
    let dir = init_vault();

    // Create a markdown file with frontmatter matching Document serialization format
    let md_content = r#"---
id: proj-test-001
type: project
title: Test Project
observed_at: "2025-02-10T00:00:00Z"
valid_until: "2025-08-10T00:00:00Z"
temporal_precision: day
_created_at: "2025-02-10T00:00:00Z"
_modified_at: "2025-02-10T00:00:00Z"
confidence: 0.95
tags:
  - rust
  - test
---
# Test Project

This is a test project document.
"#;
    let file_path = dir.path().join("test.md");
    std::fs::write(&file_path, md_content).unwrap();

    let output = mkb_in(dir.path())
        .args([
            "add",
            "--doc-type",
            "project",
            "--title",
            "ignored",
            "--observed-at",
            "2025-02-10T00:00:00Z",
            "--from-file",
            file_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "add from-file failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["id"], "proj-test-001");
    assert_eq!(result["title"], "Test Project");
}

// === T-300.3: Query ===

#[test]
fn e2e_query_with_mkql() {
    let dir = init_vault();
    add_project(dir.path(), "Alpha Project");
    add_project(dir.path(), "Beta Project");

    let output = mkb_in(dir.path())
        .args(["query", "SELECT * FROM project"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Alpha Project"));
    assert!(stdout.contains("Beta Project"));
}

#[test]
fn e2e_query_with_format_flag() {
    let dir = init_vault();
    add_project(dir.path(), "Alpha Project");

    // Table format
    let output = mkb_in(dir.path())
        .args(["query", "SELECT * FROM project", "--format", "table"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("---")); // Table separator

    // Markdown format
    let output = mkb_in(dir.path())
        .args(["query", "SELECT * FROM project", "--format", "markdown"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("|")); // Markdown table pipes
}

#[test]
fn e2e_query_pipe_to_stdout() {
    let dir = init_vault();
    add_project(dir.path(), "Alpha Project");

    let output = mkb_in(dir.path())
        .args(["query", "--doc-type", "project"])
        .output()
        .unwrap();
    assert!(output.status.success());
    // Should produce valid JSON to stdout
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(parsed.is_array());
}

// === T-300.4: Search ===

#[test]
fn e2e_search_fulltext() {
    let dir = init_vault();

    // Add documents with different bodies
    let output = mkb_in(dir.path())
        .args([
            "add",
            "--doc-type",
            "project",
            "--title",
            "ML Project",
            "--observed-at",
            "2025-02-10T00:00:00Z",
            "--body",
            "This project uses machine learning and neural networks",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = mkb_in(dir.path())
        .args(["search", "machine learning"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ML Project"));
}

// === T-300.5: Edit + Rm ===

#[test]
fn e2e_edit_updates_fields() {
    let dir = init_vault();
    let added = add_project(dir.path(), "Original Title");
    let doc_id = added["id"].as_str().unwrap();

    let output = mkb_in(dir.path())
        .args(["edit", doc_id, "--title", "Updated Title"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "edit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["title"], "Updated Title");
}

#[test]
fn e2e_rm_soft_delete() {
    let dir = init_vault();
    let added = add_project(dir.path(), "To Delete");
    let doc_id = added["id"].as_str().unwrap();

    let output = mkb_in(dir.path())
        .args(["rm", doc_id, "--doc-type", "project"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "rm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result["archived_to"].as_str().unwrap().contains("archive"));
}

// === T-300.6: Link ===

#[test]
fn e2e_link_create_and_list() {
    let dir = init_vault();
    let alpha = add_project(dir.path(), "Alpha");
    let beta = add_project(dir.path(), "Beta");
    let alpha_id = alpha["id"].as_str().unwrap();
    let beta_id = beta["id"].as_str().unwrap();

    // Create link
    let output = mkb_in(dir.path())
        .args([
            "link",
            "create",
            "--source",
            alpha_id,
            "--rel",
            "depends_on",
            "--target",
            beta_id,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "link create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // List forward links
    let output = mkb_in(dir.path())
        .args(["link", "list", alpha_id])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("depends_on"));

    // List reverse links
    let output = mkb_in(dir.path())
        .args(["link", "list", beta_id, "--reverse"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("depends_on"));
}

// === T-300.7: Schema ===

#[test]
fn e2e_schema_list() {
    let output = mkb().arg("schema").arg("list").output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("project"));
    assert!(stdout.contains("meeting"));
    assert!(stdout.contains("decision"));
    assert!(stdout.contains("signal"));
}

// === T-300.8: GC ===

#[test]
fn e2e_gc_sweep() {
    let dir = init_vault();
    add_project(dir.path(), "Test Project");

    let output = mkb_in(dir.path()).args(["gc"]).output().unwrap();
    assert!(
        output.status.success(),
        "gc failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result["swept_at"].as_str().is_some());
    assert!(result["stale_count"].is_number());
}

// === T-300.9: Stats ===

#[test]
fn e2e_stats_shows_vault_summary() {
    let dir = init_vault();
    add_project(dir.path(), "Alpha");
    add_project(dir.path(), "Beta");

    let output = mkb_in(dir.path()).args(["stats"]).output().unwrap();
    assert!(output.status.success());

    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["indexed_documents"], 2);
    assert!(result["by_type"]["project"].as_u64().unwrap() >= 2);
}

// === T-300.10: Status ===

#[test]
fn e2e_status_shows_health() {
    let dir = init_vault();
    add_project(dir.path(), "Test");

    let output = mkb_in(dir.path()).args(["status"]).output().unwrap();
    assert!(output.status.success());

    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["indexed_documents"], 1);
    assert!(result["rejection_count"].is_number());
    assert!(result["index_synced"].is_boolean());
}

// === T-300.11: Ingest ===

#[test]
fn e2e_ingest_file() {
    let dir = init_vault();

    // Create a plain markdown file (no frontmatter)
    let md_content = "# My Notes\n\nSome important notes about the project.\n";
    let file_path = dir.path().join("notes.md");
    std::fs::write(&file_path, md_content).unwrap();

    let output = mkb_in(dir.path())
        .args(["ingest", file_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ingest failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["ingested"], 1);
    assert_eq!(result["rejected"], 0);
}
