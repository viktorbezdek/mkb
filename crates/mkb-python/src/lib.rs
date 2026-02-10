//! # mkb-python
//!
//! PyO3 bridge for MKB. Thin translation layer exposing Rust functionality
//! to Python. No business logic here — just type conversion and FFI.
//!
//! All functions take vault_path as first argument (path-based API,
//! no persistent handles across FFI boundary).

use std::path::Path;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use chrono::{DateTime, Utc};

use mkb_core::document::Document;
use mkb_core::temporal::{DecayProfile, RawTemporalInput, TemporalGate, TemporalPrecision};
use mkb_index::IndexManager;
use mkb_vault::Vault;

// === Helpers ===

fn open_index(vault_path: &Path) -> PyResult<IndexManager> {
    let index_path = vault_path.join(".mkb").join("index").join("mkb.db");
    IndexManager::open(&index_path).map_err(|e| PyValueError::new_err(format!("Index error: {e}")))
}

fn parse_precision(s: &str) -> PyResult<TemporalPrecision> {
    match s.to_lowercase().as_str() {
        "exact" => Ok(TemporalPrecision::Exact),
        "day" => Ok(TemporalPrecision::Day),
        "week" => Ok(TemporalPrecision::Week),
        "month" => Ok(TemporalPrecision::Month),
        "quarter" => Ok(TemporalPrecision::Quarter),
        "approximate" | "approx" => Ok(TemporalPrecision::Approximate),
        "inferred" => Ok(TemporalPrecision::Inferred),
        other => Err(PyValueError::new_err(format!("Unknown precision: {other}"))),
    }
}

fn parse_datetime(s: &str) -> PyResult<DateTime<Utc>> {
    s.parse::<DateTime<Utc>>()
        .map_err(|e| PyValueError::new_err(format!("Invalid datetime '{s}': {e}")))
}

fn doc_to_dict(py: Python<'_>, doc: &Document) -> PyResult<Py<PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("id", &doc.id)?;
    dict.set_item("type", &doc.doc_type)?;
    dict.set_item("title", &doc.title)?;
    dict.set_item("body", &doc.body)?;
    dict.set_item("observed_at", doc.temporal.observed_at.to_rfc3339())?;
    dict.set_item("valid_until", doc.temporal.valid_until.to_rfc3339())?;
    dict.set_item(
        "temporal_precision",
        format!("{:?}", doc.temporal.temporal_precision).to_lowercase(),
    )?;
    dict.set_item("confidence", doc.confidence)?;
    dict.set_item("created_at", doc.created_at.to_rfc3339())?;
    dict.set_item("modified_at", doc.modified_at.to_rfc3339())?;
    dict.set_item("tags", &doc.tags)?;
    dict.set_item("source", &doc.source)?;
    Ok(dict.into())
}

// === Vault Operations (T-400.1) ===

/// Initialize a new MKB vault at the given path.
#[pyfunction]
fn init_vault(path: &str) -> PyResult<String> {
    let vault_path = Path::new(path);
    let vault =
        Vault::init(vault_path).map_err(|e| PyValueError::new_err(format!("Init failed: {e}")))?;
    let index_path = vault_path.join(".mkb").join("index").join("mkb.db");
    let _index = IndexManager::open(&index_path)
        .map_err(|e| PyValueError::new_err(format!("Index creation failed: {e}")))?;

    Ok(vault
        .root()
        .canonicalize()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.to_string()))
}

/// Create a new document in the vault.
#[pyfunction]
#[pyo3(signature = (vault_path, doc_type, title, observed_at, body="", tags=None, precision="day", valid_until=None))]
#[allow(clippy::too_many_arguments)]
fn create_document(
    py: Python<'_>,
    vault_path: &str,
    doc_type: &str,
    title: &str,
    observed_at: &str,
    body: &str,
    tags: Option<Vec<String>>,
    precision: &str,
    valid_until: Option<&str>,
) -> PyResult<Py<PyDict>> {
    let vpath = Path::new(vault_path);
    let vault =
        Vault::open(vpath).map_err(|e| PyValueError::new_err(format!("Vault error: {e}")))?;
    let index = open_index(vpath)?;

    let observed = parse_datetime(observed_at)?;
    let valid = valid_until.map(parse_datetime).transpose()?;
    let prec = parse_precision(precision)?;
    let profile = DecayProfile::default_profile();

    let counter = mkb_vault::next_counter(vpath, doc_type, &mkb_vault::slugify(title));
    let id = Document::generate_id(doc_type, title, counter);

    let input = RawTemporalInput {
        observed_at: Some(observed),
        valid_until: valid,
        temporal_precision: Some(prec),
        occurred_at: None,
    };

    let mut doc = Document::new(id, doc_type.to_string(), title.to_string(), input, &profile)
        .map_err(|e| PyValueError::new_err(format!("Temporal gate rejected: {e}")))?;

    doc.body = body.to_string();
    if let Some(t) = tags {
        doc.tags = t;
    }

    let _path = vault
        .create(&doc)
        .map_err(|e| PyValueError::new_err(format!("Create failed: {e}")))?;
    index
        .index_document(&doc)
        .map_err(|e| PyValueError::new_err(format!("Index failed: {e}")))?;

    doc_to_dict(py, &doc)
}

/// Read a document from the vault.
#[pyfunction]
fn read_document(
    py: Python<'_>,
    vault_path: &str,
    doc_type: &str,
    id: &str,
) -> PyResult<Py<PyDict>> {
    let vpath = Path::new(vault_path);
    let vault =
        Vault::open(vpath).map_err(|e| PyValueError::new_err(format!("Vault error: {e}")))?;

    let doc = vault
        .read(doc_type, id)
        .map_err(|e| PyValueError::new_err(format!("Read failed: {e}")))?;

    doc_to_dict(py, &doc)
}

/// Delete a document (soft delete to archive).
#[pyfunction]
fn delete_document(vault_path: &str, doc_type: &str, id: &str) -> PyResult<String> {
    let vpath = Path::new(vault_path);
    let vault =
        Vault::open(vpath).map_err(|e| PyValueError::new_err(format!("Vault error: {e}")))?;
    let index = open_index(vpath)?;

    let archive_path = vault
        .delete(doc_type, id)
        .map_err(|e| PyValueError::new_err(format!("Delete failed: {e}")))?;
    index
        .remove_document(id)
        .map_err(|e| PyValueError::new_err(format!("Index removal failed: {e}")))?;

    Ok(archive_path.display().to_string())
}

// === Index Operations (T-400.2) ===

/// Search documents using full-text search.
#[pyfunction]
fn search_fts(py: Python<'_>, vault_path: &str, query: &str) -> PyResult<Vec<Py<PyDict>>> {
    let index = open_index(Path::new(vault_path))?;

    let results = index
        .search_fts(query)
        .map_err(|e| PyValueError::new_err(format!("Search failed: {e}")))?;

    results
        .iter()
        .map(|r| {
            let dict = PyDict::new(py);
            dict.set_item("id", &r.id)?;
            dict.set_item("title", &r.title)?;
            dict.set_item("type", &r.doc_type)?;
            dict.set_item("rank", r.rank)?;
            Ok(dict.into())
        })
        .collect()
}

/// Execute an MKQL query and return results as JSON string.
#[pyfunction]
#[pyo3(signature = (vault_path, mkql, format="json"))]
fn query_mkql(vault_path: &str, mkql: &str, format: &str) -> PyResult<String> {
    let index = open_index(Path::new(vault_path))?;

    let ast = mkb_parser::parse_mkql(mkql)
        .map_err(|e| PyValueError::new_err(format!("Parse error: {e}")))?;
    let compiled = mkb_query::compile(&ast)
        .map_err(|e| PyValueError::new_err(format!("Compile error: {e}")))?;
    let result = mkb_query::execute(&index, &compiled)
        .map_err(|e| PyValueError::new_err(format!("Execution error: {e}")))?;

    let output_format = match format.to_lowercase().as_str() {
        "json" => mkb_query::OutputFormat::Json,
        "table" => mkb_query::OutputFormat::Table,
        "markdown" | "md" => mkb_query::OutputFormat::Markdown,
        other => {
            return Err(PyValueError::new_err(format!(
                "Unknown format: {other}. Valid: json, table, markdown"
            )))
        }
    };

    Ok(mkb_query::format_results(&result, output_format))
}

/// Query all documents in the vault.
#[pyfunction]
fn query_all(py: Python<'_>, vault_path: &str) -> PyResult<Vec<Py<PyDict>>> {
    let index = open_index(Path::new(vault_path))?;

    let results = index
        .query_all()
        .map_err(|e| PyValueError::new_err(format!("Query failed: {e}")))?;

    results
        .iter()
        .map(|r| {
            let dict = PyDict::new(py);
            dict.set_item("id", &r.id)?;
            dict.set_item("type", &r.doc_type)?;
            dict.set_item("title", &r.title)?;
            dict.set_item("observed_at", &r.observed_at)?;
            dict.set_item("valid_until", &r.valid_until)?;
            dict.set_item("confidence", r.confidence)?;
            Ok(dict.into())
        })
        .collect()
}

/// Query documents by type.
#[pyfunction]
fn query_by_type(py: Python<'_>, vault_path: &str, doc_type: &str) -> PyResult<Vec<Py<PyDict>>> {
    let index = open_index(Path::new(vault_path))?;

    let results = index
        .query_by_type(doc_type)
        .map_err(|e| PyValueError::new_err(format!("Query failed: {e}")))?;

    results
        .iter()
        .map(|r| {
            let dict = PyDict::new(py);
            dict.set_item("id", &r.id)?;
            dict.set_item("type", &r.doc_type)?;
            dict.set_item("title", &r.title)?;
            dict.set_item("observed_at", &r.observed_at)?;
            dict.set_item("valid_until", &r.valid_until)?;
            dict.set_item("confidence", r.confidence)?;
            Ok(dict.into())
        })
        .collect()
}

// === Temporal Gate (T-400.3) ===

/// Validate temporal fields without creating a document.
/// Returns a dict with validation result.
#[pyfunction]
#[pyo3(signature = (observed_at=None, valid_until=None, precision="day"))]
fn validate_temporal(
    py: Python<'_>,
    observed_at: Option<&str>,
    valid_until: Option<&str>,
    precision: &str,
) -> PyResult<Py<PyDict>> {
    let dict = PyDict::new(py);

    let obs = observed_at.map(parse_datetime).transpose()?;
    let valid = valid_until.map(parse_datetime).transpose()?;
    let prec = parse_precision(precision)?;

    let input = RawTemporalInput {
        observed_at: obs,
        valid_until: valid,
        temporal_precision: Some(prec),
        occurred_at: None,
    };
    let profile = DecayProfile::default_profile();

    match TemporalGate::validate(&input, &profile) {
        Ok(fields) => {
            dict.set_item("valid", true)?;
            dict.set_item("observed_at", fields.observed_at.to_rfc3339())?;
            dict.set_item("valid_until", fields.valid_until.to_rfc3339())?;
            dict.set_item(
                "temporal_precision",
                format!("{:?}", fields.temporal_precision).to_lowercase(),
            )?;
        }
        Err(e) => {
            dict.set_item("valid", false)?;
            dict.set_item("error", e.to_string())?;
        }
    }

    Ok(dict.into())
}

/// Get count of indexed documents.
#[pyfunction]
fn document_count(vault_path: &str) -> PyResult<u64> {
    let index = open_index(Path::new(vault_path))?;
    index
        .count()
        .map_err(|e| PyValueError::new_err(format!("Count failed: {e}")))
}

/// Get vault status (rejection count, index health).
#[pyfunction]
fn vault_status(py: Python<'_>, vault_path: &str) -> PyResult<Py<PyDict>> {
    let vpath = Path::new(vault_path);
    let vault =
        Vault::open(vpath).map_err(|e| PyValueError::new_err(format!("Vault error: {e}")))?;
    let index = open_index(vpath)?;

    let doc_count = index
        .count()
        .map_err(|e| PyValueError::new_err(format!("Count failed: {e}")))?;
    let rejection_count = vault.rejection_count().unwrap_or(0);
    let files = vault.list_documents().unwrap_or_default();

    let dict = PyDict::new(py);
    dict.set_item("vault_root", vault.root().display().to_string())?;
    dict.set_item("indexed_documents", doc_count)?;
    dict.set_item("vault_files", files.len())?;
    dict.set_item("index_synced", files.len() as u64 == doc_count)?;
    dict.set_item("rejection_count", rejection_count)?;
    Ok(dict.into())
}

// === Embedding Operations (T-410) ===

/// Store an embedding vector for a document.
#[pyfunction]
fn store_embedding(
    vault_path: &str,
    doc_id: &str,
    embedding: Vec<f32>,
    model: &str,
) -> PyResult<()> {
    let index = open_index(Path::new(vault_path))?;
    index
        .store_embedding(doc_id, &embedding, model)
        .map_err(|e| PyValueError::new_err(format!("Store embedding failed: {e}")))
}

/// Search for similar documents using vector similarity.
#[pyfunction]
#[pyo3(signature = (vault_path, query_embedding, limit=10))]
fn search_semantic(
    py: Python<'_>,
    vault_path: &str,
    query_embedding: Vec<f32>,
    limit: usize,
) -> PyResult<Vec<Py<PyDict>>> {
    let index = open_index(Path::new(vault_path))?;

    let results = index
        .search_semantic(&query_embedding, limit)
        .map_err(|e| PyValueError::new_err(format!("Semantic search failed: {e}")))?;

    results
        .iter()
        .map(|r| {
            let dict = PyDict::new(py);
            dict.set_item("id", &r.id)?;
            dict.set_item("title", &r.title)?;
            dict.set_item("type", &r.doc_type)?;
            dict.set_item("distance", r.distance)?;
            Ok(dict.into())
        })
        .collect()
}

/// Check if a document has an embedding.
#[pyfunction]
fn has_embedding(vault_path: &str, doc_id: &str) -> PyResult<bool> {
    let index = open_index(Path::new(vault_path))?;
    index
        .has_embedding(doc_id)
        .map_err(|e| PyValueError::new_err(format!("Has embedding check failed: {e}")))
}

/// Get count of documents with embeddings.
#[pyfunction]
fn embedding_count(vault_path: &str) -> PyResult<u64> {
    let index = open_index(Path::new(vault_path))?;
    index
        .embedding_count()
        .map_err(|e| PyValueError::new_err(format!("Embedding count failed: {e}")))
}

/// Get the expected embedding dimension.
#[pyfunction]
fn embedding_dim() -> usize {
    mkb_index::EMBEDDING_DIM
}

/// MKB Python module.
#[pymodule]
fn _mkb_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    // Vault CRUD (T-400.1)
    m.add_function(wrap_pyfunction!(init_vault, m)?)?;
    m.add_function(wrap_pyfunction!(create_document, m)?)?;
    m.add_function(wrap_pyfunction!(read_document, m)?)?;
    m.add_function(wrap_pyfunction!(delete_document, m)?)?;

    // Index operations (T-400.2)
    m.add_function(wrap_pyfunction!(search_fts, m)?)?;
    m.add_function(wrap_pyfunction!(query_mkql, m)?)?;
    m.add_function(wrap_pyfunction!(query_all, m)?)?;
    m.add_function(wrap_pyfunction!(query_by_type, m)?)?;

    // Temporal gate (T-400.3)
    m.add_function(wrap_pyfunction!(validate_temporal, m)?)?;

    // Embedding operations (T-410)
    m.add_function(wrap_pyfunction!(store_embedding, m)?)?;
    m.add_function(wrap_pyfunction!(search_semantic, m)?)?;
    m.add_function(wrap_pyfunction!(has_embedding, m)?)?;
    m.add_function(wrap_pyfunction!(embedding_count, m)?)?;
    m.add_function(wrap_pyfunction!(embedding_dim, m)?)?;

    // Utility
    m.add_function(wrap_pyfunction!(document_count, m)?)?;
    m.add_function(wrap_pyfunction!(vault_status, m)?)?;

    Ok(())
}
