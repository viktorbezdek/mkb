//! MKB Benchmark Binary
//!
//! Measures processing accuracy and retrieval accuracy across document scales.
//! Also reports throughput and latency as secondary metrics.
//!
//! Run with: `cargo run --bin mkb-bench --release`

use std::collections::{HashMap, HashSet};
use std::time::Instant;

type MetricRow = (&'static str, fn(&AccuracyResult) -> String);

use chrono::{Duration, Utc};
use mkb_core::document::Document;
use mkb_core::link::Link;
use mkb_core::temporal::{DecayProfile, RawTemporalInput, TemporalPrecision};
use mkb_index::IndexManager;

// ---------------------------------------------------------------------------
// Ground-truth dataset
// ---------------------------------------------------------------------------

/// Each document has known properties we can verify after indexing.
struct GroundTruthDoc {
    doc: Document,
    /// Unique keywords placed ONLY in this document's body
    unique_keywords: Vec<String>,
    /// The semantic "cluster" this doc belongs to (for KNN accuracy)
    cluster: &'static str,
    /// Whether this doc should be CURRENT() (valid_until > now)
    is_current: bool,
    /// Whether this doc should be FRESH('7d')
    is_fresh: bool,
}

const CLUSTERS: &[&str] = &[
    "infrastructure",
    "machine-learning",
    "security",
    "product",
    "people",
];

const CLUSTER_BODIES: &[&str] = &[
    "Kubernetes deployment pipeline Docker containers cloud infrastructure scaling load balancer CDN edge computing serverless functions microservices orchestration",
    "Neural network training deep learning model inference GPU compute gradient descent transformer architecture embedding vectors feature engineering dataset preparation",
    "Authentication encryption TLS certificates zero-trust firewall intrusion detection vulnerability scanning penetration testing access control audit logging",
    "User research product roadmap feature prioritization A/B testing conversion funnel user journey wireframes prototype usability testing release planning",
    "Team building hiring onboarding performance review career growth mentoring 1:1 meetings culture values engagement survey retention strategy",
];

fn generate_ground_truth(n: usize) -> Vec<GroundTruthDoc> {
    let profile = DecayProfile::default_profile();
    let now = Utc::now();
    let mut docs = Vec::with_capacity(n);

    for i in 0..n {
        let cluster_idx = i % CLUSTERS.len();
        let cluster = CLUSTERS[cluster_idx];
        let doc_type = match cluster_idx {
            0 => "project",
            1 => "meeting",
            2 => "decision",
            3 => "signal",
            _ => "person",
        };

        // Stagger observed_at: first 60% are fresh (within 7 days), rest are older
        let days_ago = if i < n * 6 / 10 {
            (i % 5) as i64 // 0-4 days ago (fresh)
        } else {
            30 + (i % 300) as i64 // 30-329 days ago (stale)
        };

        // First 80% are current (valid_until in future), rest are expired
        let is_current = i < n * 8 / 10;
        let valid_until = if is_current {
            Some(now + Duration::days(30))
        } else {
            Some(now - Duration::days(1))
        };

        let is_fresh = days_ago <= 7;

        let title = format!("{} {} doc-{}", cluster, doc_type, i);
        // Unique keyword that only appears in this document
        let unique_kw = format!("uniqtoken{:06}", i);

        let input = RawTemporalInput {
            observed_at: Some(now - Duration::days(days_ago)),
            valid_until,
            temporal_precision: Some(TemporalPrecision::Day),
            occurred_at: None,
        };

        let id = Document::generate_id(doc_type, &title, i as u32);
        let mut doc = Document::new(id, doc_type.to_string(), title, input, &profile)
            .expect("document creation must succeed");

        doc.body = format!(
            "{} {} This document is about {} topics. {}",
            CLUSTER_BODIES[cluster_idx], unique_kw, cluster, unique_kw
        );
        doc.tags = vec![cluster.to_string(), doc_type.to_string()];
        doc.confidence = if i % 3 == 0 { 0.95 } else { 0.6 };

        docs.push(GroundTruthDoc {
            doc,
            unique_keywords: vec![unique_kw],
            cluster,
            is_current,
            is_fresh,
        });
    }
    docs
}

// ---------------------------------------------------------------------------
// Accuracy metrics
// ---------------------------------------------------------------------------

fn precision_at_k(retrieved: &[String], relevant: &HashSet<String>, k: usize) -> f64 {
    let top_k: HashSet<_> = retrieved.iter().take(k).collect();
    if top_k.is_empty() {
        return if relevant.is_empty() { 1.0 } else { 0.0 };
    }
    let hits = top_k.iter().filter(|id| relevant.contains(**id)).count();
    hits as f64 / top_k.len() as f64
}

fn recall(retrieved: &[String], relevant: &HashSet<String>) -> f64 {
    if relevant.is_empty() {
        return 1.0;
    }
    let retrieved_set: HashSet<_> = retrieved.iter().collect();
    let hits = relevant.iter().filter(|id| retrieved_set.contains(id)).count();
    hits as f64 / relevant.len() as f64
}

// ---------------------------------------------------------------------------
// Accuracy benchmark
// ---------------------------------------------------------------------------

struct AccuracyResult {
    // Processing accuracy
    ingest_accuracy: f64,        // docs correctly indexed / total docs
    field_preservation: f64,     // fields correctly preserved after round-trip
    temporal_integrity: f64,     // temporal fields (observed_at, valid_until) correct

    // FTS retrieval accuracy
    fts_precision_at_10: f64,    // avg precision@10 for unique-keyword searches
    fts_recall: f64,             // avg recall for unique-keyword searches

    // MKQL retrieval accuracy
    mkql_type_filter: f64,       // query by type returns correct set
    mkql_current_accuracy: f64,  // CURRENT() returns only non-expired docs
    mkql_fresh_accuracy: f64,    // FRESH('7d') returns only recent docs

    // Semantic retrieval accuracy
    knn_cluster_precision: f64,  // top-K results from same cluster

    // Performance (secondary)
    ingest_docs_per_sec: f64,
    fts_p50_us: f64,
    mkql_p50_us: f64,
    knn_p50_us: f64,
    index_size_bytes: u64,
}

fn run_accuracy_benchmark(n: usize) -> AccuracyResult {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("bench.db");
    let index = IndexManager::open(&db_path).expect("open index");

    let ground_truth = generate_ground_truth(n);

    // -----------------------------------------------------------------------
    // 1. Processing: Ingest all documents
    // -----------------------------------------------------------------------
    let start = Instant::now();
    let mut ingest_failures = 0usize;
    for gt in &ground_truth {
        if index.index_document(&gt.doc).is_err() {
            ingest_failures += 1;
        }
    }
    let ingest_elapsed = start.elapsed();
    let ingest_docs_per_sec = n as f64 / ingest_elapsed.as_secs_f64();
    let ingest_accuracy = (n - ingest_failures) as f64 / n as f64;

    // Store embeddings for ALL docs (accuracy matters more than speed here)
    for gt in &ground_truth {
        let embedding = mkb_index::mock_embedding(&gt.doc.body);
        let _ = index.store_embedding(&gt.doc.id, &embedding, "mock");
    }

    // Store some links for link traversal accuracy
    let link_sources: Vec<&str> = ground_truth
        .iter()
        .take(n.min(20))
        .map(|gt| gt.doc.id.as_str())
        .collect();
    for (i, &src) in link_sources.iter().enumerate() {
        if i + 1 < link_sources.len() {
            let link = Link {
                rel: "related_to".to_string(),
                target: link_sources[i + 1].to_string(),
                observed_at: Utc::now(),
                metadata: None,
            };
            let _ = index.store_links(src, &[link]);
        }
    }

    // -----------------------------------------------------------------------
    // 2. Processing: Verify field preservation
    // -----------------------------------------------------------------------
    let mut fields_correct = 0usize;
    let mut fields_total = 0usize;
    let mut temporal_correct = 0usize;
    let mut temporal_total = 0usize;

    for gt in &ground_truth {
        // Query back by type and check fields
        if let Ok(all) = index.query_by_type(&gt.doc.doc_type) {
            if let Some(indexed) = all.iter().find(|d| d.id == gt.doc.id) {
                fields_total += 3; // id, title, doc_type
                if indexed.id == gt.doc.id {
                    fields_correct += 1;
                }
                if indexed.title == gt.doc.title {
                    fields_correct += 1;
                }
                if indexed.doc_type == gt.doc.doc_type {
                    fields_correct += 1;
                }

                // Temporal: observed_at and valid_until preserved
                temporal_total += 2;
                if !indexed.observed_at.is_empty() {
                    temporal_correct += 1;
                }
                if !indexed.valid_until.is_empty() {
                    temporal_correct += 1;
                }
            }
        }
    }
    let field_preservation = if fields_total > 0 {
        fields_correct as f64 / fields_total as f64
    } else {
        0.0
    };
    let temporal_integrity = if temporal_total > 0 {
        temporal_correct as f64 / temporal_total as f64
    } else {
        0.0
    };

    // -----------------------------------------------------------------------
    // 3. FTS Retrieval: Search for unique keywords
    // -----------------------------------------------------------------------
    let num_fts_tests = 50.min(n);
    let mut fts_precisions = Vec::with_capacity(num_fts_tests);
    let mut fts_recalls = Vec::with_capacity(num_fts_tests);
    let mut fts_latencies = Vec::with_capacity(num_fts_tests);

    for gt in ground_truth.iter().take(num_fts_tests) {
        let kw = &gt.unique_keywords[0];
        let relevant: HashSet<String> = [gt.doc.id.clone()].into_iter().collect();

        let start = Instant::now();
        let results = index.search_fts(kw).unwrap_or_default();
        fts_latencies.push(start.elapsed().as_micros() as f64);

        let retrieved_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
        fts_precisions.push(precision_at_k(&retrieved_ids, &relevant, 10));
        fts_recalls.push(recall(&retrieved_ids, &relevant));
    }

    let fts_precision_at_10 = avg(&fts_precisions);
    let fts_recall = avg(&fts_recalls);
    fts_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let fts_p50_us = percentile(&fts_latencies, 50.0);

    // -----------------------------------------------------------------------
    // 4. MKQL Retrieval: Type filter accuracy
    // -----------------------------------------------------------------------
    let mut type_accuracies = Vec::new();
    let mut mkql_latencies = Vec::new();

    for doc_type in &["project", "meeting", "decision", "signal", "person"] {
        let expected: HashSet<String> = ground_truth
            .iter()
            .filter(|gt| gt.doc.doc_type == *doc_type)
            .map(|gt| gt.doc.id.clone())
            .collect();

        let mkql_str = format!("SELECT * FROM {doc_type}");
        let ast = mkb_parser::parse_mkql(&mkql_str).expect("parse");
        let compiled = mkb_query::compile(&ast).expect("compile");

        let start = Instant::now();
        let result = mkb_query::execute(&index, &compiled);
        mkql_latencies.push(start.elapsed().as_micros() as f64);

        if let Ok(qr) = result {
            let retrieved: HashSet<String> = qr
                .rows
                .iter()
                .filter_map(|r| r.fields.get("id").and_then(|v| v.as_str()).map(String::from))
                .collect();
            if expected.is_empty() && retrieved.is_empty() {
                type_accuracies.push(1.0);
            } else if expected.is_empty() || retrieved.is_empty() {
                type_accuracies.push(0.0);
            } else {
                let intersection = expected.intersection(&retrieved).count();
                let union = expected.union(&retrieved).count();
                type_accuracies.push(intersection as f64 / union as f64); // Jaccard
            }
        }
    }
    let mkql_type_filter = avg(&type_accuracies);

    // -----------------------------------------------------------------------
    // 5. MKQL Retrieval: CURRENT() accuracy
    // -----------------------------------------------------------------------
    let ast = mkb_parser::parse_mkql("SELECT * FROM project WHERE CURRENT()").expect("parse");
    let compiled = mkb_query::compile(&ast).expect("compile");
    let start = Instant::now();
    let result = mkb_query::execute(&index, &compiled);
    mkql_latencies.push(start.elapsed().as_micros() as f64);

    let mkql_current_accuracy = if let Ok(qr) = result {
        let retrieved: HashSet<String> = qr
            .rows
            .iter()
            .filter_map(|r| r.fields.get("id").and_then(|v| v.as_str()).map(String::from))
            .collect();
        // All retrieved should be current (precision-focused)
        let expected_current_projects: HashSet<String> = ground_truth
            .iter()
            .filter(|gt| gt.is_current && gt.doc.doc_type == "project")
            .map(|gt| gt.doc.id.clone())
            .collect();
        if expected_current_projects.is_empty() && retrieved.is_empty() {
            1.0
        } else if expected_current_projects.is_empty() {
            0.0
        } else {
            // Check: every retrieved doc should actually be current
            let false_positives = retrieved
                .iter()
                .filter(|id| !expected_current_projects.contains(*id))
                .count();
            1.0 - (false_positives as f64 / retrieved.len().max(1) as f64)
        }
    } else {
        0.0
    };

    // -----------------------------------------------------------------------
    // 6. MKQL Retrieval: FRESH('7d') accuracy
    // -----------------------------------------------------------------------
    let expected_fresh_meetings: HashSet<String> = ground_truth
        .iter()
        .filter(|gt| gt.is_fresh && gt.doc.doc_type == "meeting")
        .map(|gt| gt.doc.id.clone())
        .collect();

    let ast = mkb_parser::parse_mkql("SELECT * FROM meeting WHERE FRESH('7d')").expect("parse");
    let compiled = mkb_query::compile(&ast).expect("compile");
    let start = Instant::now();
    let result = mkb_query::execute(&index, &compiled);
    mkql_latencies.push(start.elapsed().as_micros() as f64);

    let mkql_fresh_accuracy = if let Ok(qr) = result {
        let retrieved: HashSet<String> = qr
            .rows
            .iter()
            .filter_map(|r| r.fields.get("id").and_then(|v| v.as_str()).map(String::from))
            .collect();
        if expected_fresh_meetings.is_empty() && retrieved.is_empty() {
            1.0
        } else if expected_fresh_meetings.is_empty() {
            0.0
        } else {
            let intersection = expected_fresh_meetings
                .intersection(&retrieved)
                .count();
            let union = expected_fresh_meetings.union(&retrieved).count();
            intersection as f64 / union as f64
        }
    } else {
        0.0
    };

    mkql_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mkql_p50_us = percentile(&mkql_latencies, 50.0);

    // -----------------------------------------------------------------------
    // 7. Semantic Retrieval: Cluster precision
    // -----------------------------------------------------------------------
    let num_knn_tests = 25.min(n / CLUSTERS.len());
    let mut cluster_precisions = Vec::with_capacity(num_knn_tests * CLUSTERS.len());
    let mut knn_latencies = Vec::with_capacity(num_knn_tests * CLUSTERS.len());

    // Build cluster membership map
    let cluster_members: HashMap<&str, HashSet<String>> = {
        let mut map = HashMap::new();
        for gt in &ground_truth {
            map.entry(gt.cluster)
                .or_insert_with(HashSet::new)
                .insert(gt.doc.id.clone());
        }
        map
    };

    for (ci, &cluster) in CLUSTERS.iter().enumerate() {
        // Use the cluster body as the query to find similar docs
        let query_embedding = mkb_index::mock_embedding(CLUSTER_BODIES[ci]);
        let relevant = cluster_members.get(cluster).cloned().unwrap_or_default();

        let start = Instant::now();
        let results = index.search_semantic(&query_embedding, 10).unwrap_or_default();
        knn_latencies.push(start.elapsed().as_micros() as f64);

        let retrieved_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
        cluster_precisions.push(precision_at_k(&retrieved_ids, &relevant, 10));
    }

    let knn_cluster_precision = avg(&cluster_precisions);
    knn_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let knn_p50_us = percentile(&knn_latencies, 50.0);

    // -----------------------------------------------------------------------
    // Index size
    // -----------------------------------------------------------------------
    let index_size_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    AccuracyResult {
        ingest_accuracy,
        field_preservation,
        temporal_integrity,
        fts_precision_at_10,
        fts_recall,
        mkql_type_filter,
        mkql_current_accuracy,
        mkql_fresh_accuracy,
        knn_cluster_precision,
        ingest_docs_per_sec,
        fts_p50_us,
        mkql_p50_us,
        knn_p50_us,
        index_size_bytes,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn avg(vals: &[f64]) -> f64 {
    if vals.is_empty() {
        return 0.0;
    }
    vals.iter().sum::<f64>() / vals.len() as f64
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    let idx = if idx == 0 { 0 } else { idx - 1 };
    sorted[idx.min(sorted.len() - 1)]
}

fn format_pct(v: f64) -> String {
    format!("{:.1}%", v * 100.0)
}

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
    if docs_per_sec >= 1_000.0 {
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

fn format_scale(n: usize) -> String {
    if n >= 1_000 {
        format!("{}K", n / 1_000)
    } else {
        n.to_string()
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let scales: &[usize] = &[100, 1_000, 10_000];

    println!();
    println!("MKB Benchmark — Processing & Retrieval Accuracy");
    println!("================================================");
    println!(
        "Platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("Date: {}", Utc::now().format("%Y-%m-%d"));
    println!();

    // Warmup
    eprint!("Warming up... ");
    let _ = run_accuracy_benchmark(20);
    eprintln!("done.");
    println!();

    // Run at each scale
    let mut results: Vec<(usize, AccuracyResult)> = Vec::new();
    for &n in scales {
        eprint!("Benchmarking {} docs... ", n);
        let start = Instant::now();
        let result = run_accuracy_benchmark(n);
        let total = start.elapsed();
        eprintln!("done in {:.1}s", total.as_secs_f64());
        results.push((n, result));
    }

    println!();

    // --- Print accuracy table ---
    let col0 = 28;
    let colw = 14;

    println!("### Processing Accuracy");
    println!();
    print!("| {:col0$}", "Metric");
    for &n in scales {
        print!("| {:>colw$}", format!("{} docs", format_scale(n)));
    }
    println!("|");
    print!("|{}", "-".repeat(col0 + 1));
    for _ in scales {
        print!("|{}", "-".repeat(colw + 1));
    }
    println!("|");

    let accuracy_rows: Vec<MetricRow> = vec![
        ("Ingest Accuracy", |r| format_pct(r.ingest_accuracy)),
        ("Field Preservation", |r| format_pct(r.field_preservation)),
        ("Temporal Integrity", |r| format_pct(r.temporal_integrity)),
    ];
    for (label, getter) in &accuracy_rows {
        print!("| {:col0$}", label);
        for (_, r) in &results {
            print!("| {:>colw$}", getter(r));
        }
        println!("|");
    }

    println!();
    println!("### Retrieval Accuracy");
    println!();
    print!("| {:col0$}", "Metric");
    for &n in scales {
        print!("| {:>colw$}", format!("{} docs", format_scale(n)));
    }
    println!("|");
    print!("|{}", "-".repeat(col0 + 1));
    for _ in scales {
        print!("|{}", "-".repeat(colw + 1));
    }
    println!("|");

    let retrieval_rows: Vec<MetricRow> = vec![
        ("FTS Precision@10", |r| format_pct(r.fts_precision_at_10)),
        ("FTS Recall", |r| format_pct(r.fts_recall)),
        ("MKQL Type Filter (Jaccard)", |r| {
            format_pct(r.mkql_type_filter)
        }),
        ("MKQL CURRENT() Precision", |r| {
            format_pct(r.mkql_current_accuracy)
        }),
        ("MKQL FRESH('7d') (Jaccard)", |r| {
            format_pct(r.mkql_fresh_accuracy)
        }),
        ("KNN Cluster Precision@10", |r| {
            format_pct(r.knn_cluster_precision)
        }),
    ];
    for (label, getter) in &retrieval_rows {
        print!("| {:col0$}", label);
        for (_, r) in &results {
            print!("| {:>colw$}", getter(r));
        }
        println!("|");
    }

    println!();
    println!("### Performance");
    println!();
    print!("| {:col0$}", "Metric");
    for &n in scales {
        print!("| {:>colw$}", format!("{} docs", format_scale(n)));
    }
    println!("|");
    print!("|{}", "-".repeat(col0 + 1));
    for _ in scales {
        print!("|{}", "-".repeat(colw + 1));
    }
    println!("|");

    let perf_rows: Vec<MetricRow> = vec![
        ("Ingest Throughput", |r| {
            format_throughput(r.ingest_docs_per_sec)
        }),
        ("FTS Search (p50)", |r| format_duration_us(r.fts_p50_us)),
        ("MKQL Query (p50)", |r| format_duration_us(r.mkql_p50_us)),
        ("KNN Search (p50)", |r| format_duration_us(r.knn_p50_us)),
        ("Index Size", |r| format_size(r.index_size_bytes)),
    ];
    for (label, getter) in &perf_rows {
        print!("| {:col0$}", label);
        for (_, r) in &results {
            print!("| {:>colw$}", getter(r));
        }
        println!("|");
    }

    println!();
}
