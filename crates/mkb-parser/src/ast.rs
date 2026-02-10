//! AST types for MKQL (Markdown Knowledge Query Language) parsed queries.
//!
//! The AST represents the parsed structure of an MKQL query.
//! It is produced by the parser and consumed by the query compiler.

use serde::{Deserialize, Serialize};

/// A complete MKQL query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MkqlQuery {
    pub select: SelectClause,
    pub from: String,
    pub where_clause: Option<WhereClause>,
    pub order_by: Option<Vec<OrderByItem>>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

/// The SELECT clause: which fields to return.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SelectClause {
    /// `SELECT *`
    Star,
    /// `SELECT field1, field2, ...`
    Fields(Vec<SelectField>),
}

/// A single field in a SELECT clause.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectField {
    pub name: String,
    pub alias: Option<String>,
}

/// The WHERE clause: a tree of predicates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WhereClause {
    Predicate(Predicate),
    And(Box<WhereClause>, Box<WhereClause>),
    Or(Box<WhereClause>, Box<WhereClause>),
    Not(Box<WhereClause>),
}

/// A single predicate in a WHERE clause.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Predicate {
    /// `field = value`, `field > value`, etc.
    Comparison {
        field: String,
        op: CompOp,
        value: Value,
    },
    /// `field IN ('a', 'b', 'c')`
    InList { field: String, values: Vec<Value> },
    /// `field LIKE 'pattern%'`
    Like { field: String, pattern: String },
    /// `BODY CONTAINS 'search term'`
    BodyContains { term: String },
    /// Temporal function predicates: `FRESH('7d')`, `CURRENT()`, etc.
    Temporal(TemporalFunction),
    /// `LINKED('rel', 'target')` or `LINKED(REVERSE, 'rel', 'source')`
    Linked(LinkedFunction),
}

/// Comparison operators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompOp {
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
}

/// Literal values in predicates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Null,
}

/// Temporal function calls in WHERE clauses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TemporalFunction {
    /// `FRESH('7d')` — observed within the given duration
    Fresh { duration: String },
    /// `STALE('30d')` — not observed within the given duration
    Stale { duration: String },
    /// `EXPIRED()` — past valid_until
    Expired,
    /// `CURRENT()` — not expired and not superseded
    Current,
    /// `LATEST()` — most recent version of each document
    Latest,
    /// `AS_OF('2025-02-10T00:00:00Z')` — state at a given point in time
    AsOf { datetime: String },
    /// `EFF_CONFIDENCE(> 0.5)` — effective confidence threshold
    EffConfidence { op: CompOp, threshold: f64 },
}

/// Link traversal functions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LinkedFunction {
    /// `LINKED('rel_type')` — forward link exists with this rel
    Forward { rel: String, target: Option<String> },
    /// `LINKED(REVERSE, 'rel_type')` — reverse link exists with this rel
    Reverse { rel: String, source: Option<String> },
}

/// An item in the ORDER BY clause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderByItem {
    pub field: String,
    pub direction: SortDirection,
}

/// Sort direction for ORDER BY.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

impl std::fmt::Display for CompOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Eq => write!(f, "="),
            Self::Neq => write!(f, "!="),
            Self::Lt => write!(f, "<"),
            Self::Lte => write!(f, "<="),
            Self::Gt => write!(f, ">"),
            Self::Gte => write!(f, ">="),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s) => write!(f, "'{s}'"),
            Self::Integer(i) => write!(f, "{i}"),
            Self::Float(fl) => write!(f, "{fl}"),
            Self::Boolean(b) => write!(f, "{b}"),
            Self::Null => write!(f, "NULL"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ast_roundtrip_simple_query() {
        let query = MkqlQuery {
            select: SelectClause::Star,
            from: "project".to_string(),
            where_clause: None,
            order_by: None,
            limit: None,
            offset: None,
        };

        let json = serde_json::to_string(&query).expect("serialize");
        let back: MkqlQuery = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(query, back);
    }

    #[test]
    fn ast_roundtrip_complex_query() {
        let query = MkqlQuery {
            select: SelectClause::Fields(vec![
                SelectField {
                    name: "title".to_string(),
                    alias: None,
                },
                SelectField {
                    name: "status".to_string(),
                    alias: Some("s".to_string()),
                },
            ]),
            from: "project".to_string(),
            where_clause: Some(WhereClause::And(
                Box::new(WhereClause::Predicate(Predicate::Comparison {
                    field: "status".to_string(),
                    op: CompOp::Eq,
                    value: Value::String("active".to_string()),
                })),
                Box::new(WhereClause::Predicate(Predicate::Temporal(
                    TemporalFunction::Current,
                ))),
            )),
            order_by: Some(vec![OrderByItem {
                field: "observed_at".to_string(),
                direction: SortDirection::Desc,
            }]),
            limit: Some(10),
            offset: Some(0),
        };

        let json = serde_json::to_string(&query).expect("serialize");
        let back: MkqlQuery = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(query, back);
    }
}
