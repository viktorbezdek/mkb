//! MKB Benchmark Binary
//!
//! Measures real operations at different document scales (100, 1,000, 10,000).
//! Run with: `cargo run --bin mkb-bench --release`

use std::time::Instant;

use chrono::{Duration, Utc};
use mkb_core::document::Document;
use mkb_core::temporal::{DecayProfile, RawTemporalInput, TemporalPrecision};
use mkb_index::IndexManager;

// ---------------------------------------------------------------------------
// Synthetic data generation
// ---------------------------------------------------------------------------

const DOC_TYPES: &[&str] = &["project", "meeting", "decision", "signal", "person"];

const SEARCH_TERMS: &[&str] = &[
    "architecture",
    "performance",
    "scalability",
    "deployment",
    "review",
    "sprint",
    "migration",
    "security",
    "testing",
    "pipeline",
    "design",
    "budget",
    "roadmap",
    "incident",
    "launch",
    "integration",
    "database",
    "frontend",
    "backend",
    "monitoring",
    "release",
    "feature",
    "refactor",
    "optimization",
    "documentation",
    "onboarding",
    "compliance",
    "encryption",
    "latency",
    "throughput",
    "consensus",
    "prototype",
    "benchmark",
    "container",
    "orchestration",
    "caching",
    "observability",
    "alerting",
    "dependency",
    "rollback",
    "provisioning",
    "scheduling",
    "partitioning",
    "replication",
    "failover",
    "throttling",
    "pagination",
    "validation",
    "serialization",
    "concurrency",
];

const BODY_FRAGMENTS: &[&str] = &[
    "The team discussed various approaches to solving the problem including architecture redesigns and performance improvements.",
    "Key decisions were made regarding the deployment strategy and scalability requirements for the next quarter.",
    "Sprint review covered progress on migration tasks, security audit findings, and testing infrastructure updates.",
    "The pipeline for continuous integration needs optimization to reduce build times and improve developer feedback loops.",
    "Design review focused on the database schema changes required for the multi-tenant feature rollout.",
    "Budget allocation for Q3 includes resources for frontend modernization, backend scaling, and monitoring infrastructure.",
    "Roadmap priorities shifted after the incident post-mortem identified gaps in our observability and alerting systems.",
    "Launch planning for the new product integration requires coordination across multiple teams and dependency management.",
    "The encryption module underwent a compliance review with recommendations for key rotation and access control improvements.",
    "Performance benchmarks revealed latency issues in the caching layer that impact throughput during peak traffic.",
];

fn generate_document(i: usize, doc_type: &str) -> Document {
    let title = format!("{} {}", doc_type, i);
    let input = RawTemporalInput {
        observed_at: Some(Utc::now() - Duration::days(i as i64 % 365)),
        valid_until: None,
        temporal_precision: Some(TemporalPrecision::Day),
        occurred_at: None,
    };
    let profile = DecayProfile::default_profile();
    let id = Document::generate_id(doc_type, &title, i as u32);
    let mut doc = Document::new(id, doc_type.to_string(), title, input, &profile)
        .expect("document creation should not fail in benchmark");
    let body_idx = i % BODY_FRAGMENTS.len();
    doc.body = format!(
        "{} Additional context for {} document number {}: {}",
        BODY_FRAGMENTS[body_idx],
        doc_type,
        i,
        SEARCH_TERMS[i % SEARCH_TERMS.len()]
    );
    doc.tags = vec![
        SEARCH_TERMS[i % SEARCH_TERMS.len()].to_string(),
        SEARCH_TERMS[(i + 7) % SEARCH_TERMS.len()].to_string(),
    ];
    doc
}

// ---------------------------------------------------------------------------
// Percentile computation
// ---------------------------------------------------------------------------

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    let idx = if idx == 0 { 0 } else { idx - 1 };
    sorted[idx.min(sorted.len() - 1)]
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn format_duration_us(us: f64) -> String {
    if us >= 1_000_000.0 {
        format!("{:.1} s", us / 1_000_000.0)
    } else if us >= 1_000.0 {
        format!("{:.0} ms", us / 1_000.0)
    } else {
        format!("{:.0} us", us)
    }
}

fn format_throughput(docs_per_sec: f64) -> String {
    if docs_per_sec >= 1_000_000.0 {
        format!("{:.1}M/s", docs_per_sec / 1_000_000.0)
    } else if docs_per_sec >= 1_000.0 {
        format!("{:.1}K/s", docs_per_sec / 1_000.0)
    } else {
        format!("{:.0}/s", docs_per_sec)
    }
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1_024 * 1_024 {
        format!("{:.1} MB", bytes as f64 / (1_024.0 * 1_024.0))
    } else {
        format!("{:.0} KB", bytes as f64 / 1_024.0)
    }
}

// ---------------------------------------------------------------------------
// Benchmark results
// ---------------------------------------------------------------------------

type MetricRow = (&'static str, fn(&ScaleResult) -> f64);

#[derive(Default)]
struct ScaleResult {
    ingest_docs_per_sec: f64,
    fts_p50_us: f64,
    fts_p95_us: f64,
    fts_p99_us: f64,
    mkql_p50_us: f64,
    mkql_p95_us: f64,
    mkql_p99_us: f64,
    knn_p50_us: f64,
    knn_p95_us: f64,
    knn_p99_us: f64,
    index_size_bytes: u64,
}

// ---------------------------------------------------------------------------
// Benchmark runner for a single scale
// ---------------------------------------------------------------------------

fn run_benchmark(n: usize) -> ScaleResult {
    let tmp = tempfile::TempDir::new().expect("failed to create temp dir");
    let db_path = tmp.path().join("bench.db");
    let index = IndexManager::open(&db_path).expect("failed to open index");

    let mut result = ScaleResult::default();

    // --- Bulk Ingest ---
    let docs: Vec<Document> = (0..n)
        .map(|i| {
            let doc_type = DOC_TYPES[i % DOC_TYPES.len()];
            generate_document(i, doc_type)
        })
        .collect();

    let start = Instant::now();
    for doc in &docs {
        index
            .index_document(doc)
            .expect("index_document should not fail");
    }
    let ingest_elapsed = start.elapsed();
    result.ingest_docs_per_sec = n as f64 / ingest_elapsed.as_secs_f64();

    // --- Store embeddings for KNN benchmark ---
    // Only embed a subset (first 20% or all if small) to keep setup time reasonable
    let embed_count = (n / 5).max(50).min(n);
    for doc in docs.iter().take(embed_count) {
        let embedding = mkb_index::mock_embedding(&doc.body);
        index
            .store_embedding(&doc.id, &embedding, "mock")
            .expect("store_embedding should not fail");
    }

    // --- FTS Search ---
    let num_searches = 50;
    let mut fts_latencies: Vec<f64> = Vec::with_capacity(num_searches);
    for i in 0..num_searches {
        let term = SEARCH_TERMS[i % SEARCH_TERMS.len()];
        let start = Instant::now();
        let _ = index.search_fts(term);
        let elapsed = start.elapsed();
        fts_latencies.push(elapsed.as_micros() as f64);
    }
    fts_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    result.fts_p50_us = percentile(&fts_latencies, 50.0);
    result.fts_p95_us = percentile(&fts_latencies, 95.0);
    result.fts_p99_us = percentile(&fts_latencies, 99.0);

    // --- MKQL Query ---
    let mkql_queries = [
        "SELECT * FROM project WHERE CURRENT()",
        "SELECT * FROM meeting ORDER BY observed_at DESC LIMIT 10",
        "SELECT title, confidence FROM decision WHERE confidence > 0.5",
        "SELECT * FROM signal WHERE FRESH('30d')",
        "SELECT * FROM person LIMIT 20",
    ];

    let mut mkql_latencies: Vec<f64> = Vec::with_capacity(num_searches);
    for i in 0..num_searches {
        let mkql_str = mkql_queries[i % mkql_queries.len()];
        let ast = mkb_parser::parse_mkql(mkql_str).expect("MKQL parse should not fail");
        let compiled = mkb_query::compile(&ast).expect("MKQL compile should not fail");

        let start = Instant::now();
        let _ = mkb_query::execute(&index, &compiled);
        let elapsed = start.elapsed();
        mkql_latencies.push(elapsed.as_micros() as f64);
    }
    mkql_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    result.mkql_p50_us = percentile(&mkql_latencies, 50.0);
    result.mkql_p95_us = percentile(&mkql_latencies, 95.0);
    result.mkql_p99_us = percentile(&mkql_latencies, 99.0);

    // --- Semantic KNN Search ---
    let mut knn_latencies: Vec<f64> = Vec::with_capacity(num_searches);
    for i in 0..num_searches {
        let query_text = SEARCH_TERMS[i % SEARCH_TERMS.len()];
        let query_embedding = mkb_index::mock_embedding(query_text);

        let start = Instant::now();
        let _ = index.search_semantic(&query_embedding, 10);
        let elapsed = start.elapsed();
        knn_latencies.push(elapsed.as_micros() as f64);
    }
    knn_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    result.knn_p50_us = percentile(&knn_latencies, 50.0);
    result.knn_p95_us = percentile(&knn_latencies, 95.0);
    result.knn_p99_us = percentile(&knn_latencies, 99.0);

    // --- Index Size ---
    result.index_size_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    result
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let scales: &[usize] = &[100, 1_000, 10_000];

    println!();
    println!("MKB Benchmark");
    println!("=============");
    println!(
        "Platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("Date: {}", Utc::now().format("%Y-%m-%d"));
    println!();

    // Warmup: run a tiny benchmark to trigger JIT/caches
    eprint!("Warming up... ");
    let _ = run_benchmark(10);
    eprintln!("done.");
    println!();

    // Run benchmarks at each scale
    let mut results: Vec<(usize, ScaleResult)> = Vec::new();
    for &n in scales {
        eprint!("Benchmarking {} docs... ", n);
        let start = Instant::now();
        let result = run_benchmark(n);
        let total = start.elapsed();
        eprintln!("done in {:.1}s", total.as_secs_f64());
        results.push((n, result));
    }

    println!();

    // --- Print results table ---
    // Column widths
    let col0 = 22;
    let colw = 14;

    // Header
    print!("| {:col0$}", "Operation");
    for &n in scales {
        print!("| {:>colw$}", format!("{} docs", format_scale(n)));
    }
    println!("|");

    // Separator
    print!("|{}", "-".repeat(col0 + 1));
    for _ in scales {
        print!("|{}", "-".repeat(colw + 1));
    }
    println!("|");

    // Bulk Ingest
    print!("| {:col0$}", "Bulk Ingest");
    for (_, r) in &results {
        print!("| {:>colw$}", format_throughput(r.ingest_docs_per_sec));
    }
    println!("|");

    // FTS Search
    let fts_rows: Vec<MetricRow> = vec![
        ("FTS Search (p50)", |r: &ScaleResult| r.fts_p50_us),
        ("FTS Search (p95)", |r: &ScaleResult| r.fts_p95_us),
        ("FTS Search (p99)", |r: &ScaleResult| r.fts_p99_us),
    ];
    for (label, getter) in &fts_rows {
        print!("| {:col0$}", label);
        for (_, r) in &results {
            print!("| {:>colw$}", format_duration_us(getter(r)));
        }
        println!("|");
    }

    // MKQL Query
    let mkql_rows: Vec<MetricRow> = vec![
        ("MKQL Query (p50)", |r: &ScaleResult| r.mkql_p50_us),
        ("MKQL Query (p95)", |r: &ScaleResult| r.mkql_p95_us),
        ("MKQL Query (p99)", |r: &ScaleResult| r.mkql_p99_us),
    ];
    for (label, getter) in &mkql_rows {
        print!("| {:col0$}", label);
        for (_, r) in &results {
            print!("| {:>colw$}", format_duration_us(getter(r)));
        }
        println!("|");
    }

    // KNN Search
    let knn_rows: Vec<MetricRow> = vec![
        ("KNN Search (p50)", |r: &ScaleResult| r.knn_p50_us),
        ("KNN Search (p95)", |r: &ScaleResult| r.knn_p95_us),
        ("KNN Search (p99)", |r: &ScaleResult| r.knn_p99_us),
    ];
    for (label, getter) in &knn_rows {
        print!("| {:col0$}", label);
        for (_, r) in &results {
            print!("| {:>colw$}", format_duration_us(getter(r)));
        }
        println!("|");
    }

    // Index Size
    print!("| {:col0$}", "Index Size");
    for (_, r) in &results {
        print!("| {:>colw$}", format_size(r.index_size_bytes));
    }
    println!("|");

    println!();
}

fn format_scale(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{}M", n / 1_000_000)
    } else if n >= 1_000 {
        format!("{}K", n / 1_000)
    } else {
        n.to_string()
    }
}
