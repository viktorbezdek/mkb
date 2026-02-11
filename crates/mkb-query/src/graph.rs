//! Graph visualization: builds and formats document relationship graphs.
//!
//! Supports DOT, Mermaid, and JSON output formats.
//! Uses BFS traversal from a center node or collects all documents of a type.

use std::collections::{HashMap, HashSet, VecDeque};

use mkb_index::IndexManager;

/// A node in the document graph.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphNode {
    pub id: String,
    pub doc_type: String,
    pub title: String,
    pub observed_at: String,
    pub confidence: f64,
}

/// An edge in the document graph.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub rel: String,
    pub observed_at: String,
}

/// A complete document graph with nodes and edges.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DocumentGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// Builds document relationship graphs from the index.
pub struct GraphBuilder;

impl GraphBuilder {
    /// Build a graph centered on a document, traversing links up to `depth` hops (BFS).
    ///
    /// # Errors
    ///
    /// Returns an error string if index queries fail.
    pub fn from_center(
        index: &IndexManager,
        center_id: &str,
        depth: u32,
    ) -> Result<DocumentGraph, String> {
        let mut nodes_map: HashMap<String, GraphNode> = HashMap::new();
        let mut edges = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, u32)> = VecDeque::new();

        queue.push_back((center_id.to_string(), 0));
        visited.insert(center_id.to_string());

        while let Some((current_id, current_depth)) = queue.pop_front() {
            // Add node if not already present
            if !nodes_map.contains_key(&current_id) {
                if let Some(doc) = index
                    .query_by_id(&current_id)
                    .map_err(|e| format!("Failed to query document {current_id}: {e}"))?
                {
                    nodes_map.insert(
                        current_id.clone(),
                        GraphNode {
                            id: doc.id,
                            doc_type: doc.doc_type,
                            title: doc.title,
                            observed_at: doc.observed_at,
                            confidence: doc.confidence,
                        },
                    );
                }
            }

            if current_depth >= depth {
                continue;
            }

            // Forward links
            let forward = index
                .query_forward_links(&current_id)
                .map_err(|e| format!("Failed to query forward links: {e}"))?;
            for link in &forward {
                edges.push(GraphEdge {
                    source: link.source_id.clone(),
                    target: link.target_id.clone(),
                    rel: link.rel.clone(),
                    observed_at: link.observed_at.clone(),
                });
                if !visited.contains(&link.target_id) {
                    visited.insert(link.target_id.clone());
                    queue.push_back((link.target_id.clone(), current_depth + 1));
                }
            }

            // Reverse links
            let reverse = index
                .query_reverse_links(&current_id)
                .map_err(|e| format!("Failed to query reverse links: {e}"))?;
            for link in &reverse {
                edges.push(GraphEdge {
                    source: link.source_id.clone(),
                    target: link.target_id.clone(),
                    rel: link.rel.clone(),
                    observed_at: link.observed_at.clone(),
                });
                if !visited.contains(&link.source_id) {
                    visited.insert(link.source_id.clone());
                    queue.push_back((link.source_id.clone(), current_depth + 1));
                }
            }
        }

        // Deduplicate edges
        let mut seen_edges: HashSet<String> = HashSet::new();
        let unique_edges: Vec<GraphEdge> = edges
            .into_iter()
            .filter(|e| {
                let key = format!("{}->{}:{}", e.source, e.target, e.rel);
                seen_edges.insert(key)
            })
            .collect();

        Ok(DocumentGraph {
            nodes: nodes_map.into_values().collect(),
            edges: unique_edges,
        })
    }

    /// Build a graph of all documents of a given type.
    ///
    /// # Errors
    ///
    /// Returns an error string if index queries fail.
    pub fn from_type(index: &IndexManager, doc_type: &str) -> Result<DocumentGraph, String> {
        let docs = index
            .query_by_type(doc_type)
            .map_err(|e| format!("Failed to query type {doc_type}: {e}"))?;

        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let node_ids: HashSet<String> = docs.iter().map(|d| d.id.clone()).collect();

        for doc in &docs {
            nodes.push(GraphNode {
                id: doc.id.clone(),
                doc_type: doc.doc_type.clone(),
                title: doc.title.clone(),
                observed_at: doc.observed_at.clone(),
                confidence: doc.confidence,
            });

            // Get links between nodes of this type
            let forward = index
                .query_forward_links(&doc.id)
                .map_err(|e| format!("Failed to query links: {e}"))?;
            for link in forward {
                if node_ids.contains(&link.target_id) {
                    edges.push(GraphEdge {
                        source: link.source_id,
                        target: link.target_id,
                        rel: link.rel,
                        observed_at: link.observed_at,
                    });
                }
            }
        }

        Ok(DocumentGraph { nodes, edges })
    }

    /// Format a graph as DOT (Graphviz) output.
    #[must_use]
    pub fn format_dot(graph: &DocumentGraph) -> String {
        let mut out = String::from("digraph mkb {\n  rankdir=LR;\n  node [shape=box];\n\n");

        for node in &graph.nodes {
            let label = node.title.replace('"', "\\\"");
            out.push_str(&format!(
                "  \"{}\" [label=\"{}\\n({})\" tooltip=\"{}\"];\n",
                node.id, label, node.doc_type, node.observed_at
            ));
        }

        out.push('\n');

        for edge in &graph.edges {
            out.push_str(&format!(
                "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
                edge.source, edge.target, edge.rel
            ));
        }

        out.push_str("}\n");
        out
    }

    /// Format a graph as Mermaid diagram.
    #[must_use]
    pub fn format_mermaid(graph: &DocumentGraph) -> String {
        let mut out = String::from("graph LR\n");

        for node in &graph.nodes {
            let label = node.title.replace('"', "'");
            out.push_str(&format!("  {}[\"{}\"]\n", node.id.replace('-', "_"), label));
        }

        out.push('\n');

        for edge in &graph.edges {
            out.push_str(&format!(
                "  {} -->|{}| {}\n",
                edge.source.replace('-', "_"),
                edge.rel,
                edge.target.replace('-', "_")
            ));
        }

        out
    }

    /// Format a graph as JSON.
    #[must_use]
    pub fn format_json(graph: &DocumentGraph) -> String {
        serde_json::to_string_pretty(graph).unwrap_or_else(|_| "{}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use mkb_core::document::Document;
    use mkb_core::link::Link;
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
        doc.body = format!("Content for {title}");
        doc
    }

    fn setup_graph_index() -> IndexManager {
        let index = IndexManager::in_memory().unwrap();

        // Create nodes
        index
            .index_document(&make_doc("proj-alpha-001", "project", "Alpha"))
            .unwrap();
        index
            .index_document(&make_doc("proj-beta-001", "project", "Beta"))
            .unwrap();
        index
            .index_document(&make_doc("pers-jane-001", "person", "Jane Smith"))
            .unwrap();
        index
            .index_document(&make_doc("meet-standup-001", "meeting", "Standup"))
            .unwrap();

        // Create links
        let links_alpha = vec![
            Link {
                rel: "owner".to_string(),
                target: "pers-jane-001".to_string(),
                observed_at: utc(2025, 2, 10),
                metadata: None,
            },
            Link {
                rel: "depends_on".to_string(),
                target: "proj-beta-001".to_string(),
                observed_at: utc(2025, 2, 10),
                metadata: None,
            },
        ];
        index.store_links("proj-alpha-001", &links_alpha).unwrap();

        let links_meeting = vec![Link {
            rel: "discussed".to_string(),
            target: "proj-alpha-001".to_string(),
            observed_at: utc(2025, 2, 10),
            metadata: None,
        }];
        index
            .store_links("meet-standup-001", &links_meeting)
            .unwrap();

        index
    }

    #[test]
    fn graph_builder_single_node() {
        let index = IndexManager::in_memory().unwrap();
        index
            .index_document(&make_doc("proj-solo-001", "project", "Solo"))
            .unwrap();

        let graph = GraphBuilder::from_center(&index, "proj-solo-001", 1).unwrap();
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.edges.len(), 0);
    }

    #[test]
    fn graph_builder_depth_1() {
        let index = setup_graph_index();
        let graph = GraphBuilder::from_center(&index, "proj-alpha-001", 1).unwrap();

        // Alpha has 2 forward links (owner->Jane, depends_on->Beta)
        // + 1 reverse link (meeting discussed->Alpha)
        // So at depth=1: Alpha + Jane + Beta + Standup = up to 4 nodes
        assert!(graph.nodes.len() >= 3, "Expected >= 3 nodes at depth 1");
        assert!(graph.edges.len() >= 2, "Expected >= 2 edges at depth 1");
    }

    #[test]
    fn graph_builder_depth_2() {
        let index = setup_graph_index();
        let graph = GraphBuilder::from_center(&index, "meet-standup-001", 2).unwrap();

        // Meeting -> Alpha (depth 1) -> Jane, Beta (depth 2)
        assert!(
            graph.nodes.len() >= 3,
            "Expected >= 3 nodes at depth 2, got {}",
            graph.nodes.len()
        );
    }

    #[test]
    fn format_dot_output() {
        let index = setup_graph_index();
        let graph = GraphBuilder::from_center(&index, "proj-alpha-001", 1).unwrap();
        let dot = GraphBuilder::format_dot(&graph);

        assert!(dot.starts_with("digraph mkb {"));
        assert!(dot.contains("rankdir=LR"));
        assert!(dot.contains("->"));
        assert!(dot.ends_with("}\n"));
    }

    #[test]
    fn format_mermaid_output() {
        let index = setup_graph_index();
        let graph = GraphBuilder::from_center(&index, "proj-alpha-001", 1).unwrap();
        let mermaid = GraphBuilder::format_mermaid(&graph);

        assert!(mermaid.starts_with("graph LR"));
        assert!(mermaid.contains("-->|"));
    }

    #[test]
    fn format_json_structure() {
        let index = setup_graph_index();
        let graph = GraphBuilder::from_center(&index, "proj-alpha-001", 1).unwrap();
        let json = GraphBuilder::format_json(&graph);

        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["nodes"].is_array());
        assert!(parsed["edges"].is_array());
    }

    #[test]
    fn graph_by_type() {
        let index = setup_graph_index();
        let graph = GraphBuilder::from_type(&index, "project").unwrap();

        assert_eq!(graph.nodes.len(), 2); // Alpha + Beta
                                          // Should include the depends_on edge between them
        assert!(
            !graph.edges.is_empty(),
            "Expected at least 1 edge between projects"
        );
    }
}
