//! MKQL AST → parameterized SQL compiler.
//!
//! Compiles an MKQL query AST into a SQL query with bound parameters.
//! All values are parameterized to prevent SQL injection.

use mkb_parser::ast::{
    CompOp, LinkedFunction, MkqlQuery, Predicate, SelectClause, SortDirection, TemporalFunction,
    Value, WhereClause,
};

/// A compiled SQL query with bound parameters.
#[derive(Debug, Clone)]
pub struct CompiledQuery {
    /// The SQL query string with `?N` placeholders.
    pub sql: String,
    /// Bound parameter values in order.
    pub params: Vec<SqlParam>,
    /// Whether this query uses FTS5 (requires join to documents_fts).
    pub uses_fts: bool,
    /// Whether this query uses the links table.
    pub uses_links: bool,
    /// Whether this query uses semantic (vector) search via NEAR().
    pub uses_semantic: bool,
    /// Semantic search parameters: (query_text, threshold).
    pub near_params: Option<(String, f64)>,
}

/// A SQL parameter value.
#[derive(Debug, Clone)]
pub enum SqlParam {
    Text(String),
    Integer(i64),
    Float(f64),
    Null,
}

/// Compile an MKQL AST into a parameterized SQL query.
///
/// # Errors
///
/// Returns a string error if the query cannot be compiled.
pub fn compile(query: &MkqlQuery) -> Result<CompiledQuery, String> {
    let mut ctx = CompileCtx::new();

    // SELECT clause
    let select_sql = compile_select(&query.select);

    // FROM clause
    let from_sql = "documents d";

    // Reserve doc_type as first parameter
    let doc_type_idx = ctx.next_param_for_type(&query.from);

    // WHERE clause
    let where_sql = if let Some(ref wc) = query.where_clause {
        let (sql, _) = compile_where(wc, &mut ctx)?;
        format!(" WHERE d.doc_type = ?{doc_type_idx} AND {sql}")
    } else {
        format!(" WHERE d.doc_type = ?{doc_type_idx}")
    };

    // JOIN for FTS5
    let fts_join = if ctx.uses_fts {
        " JOIN documents_fts f ON d.rowid = f.rowid"
    } else {
        ""
    };

    // JOIN for links
    let link_join = if ctx.uses_links {
        " JOIN links l ON d.id = l.source_id"
    } else {
        ""
    };

    // ORDER BY
    let order_sql = if let Some(ref items) = query.order_by {
        let parts: Vec<String> = items
            .iter()
            .map(|item| {
                let dir = match item.direction {
                    SortDirection::Asc => "ASC",
                    SortDirection::Desc => "DESC",
                };
                format!("d.{} {dir}", item.field)
            })
            .collect();
        format!(" ORDER BY {}", parts.join(", "))
    } else {
        " ORDER BY d.observed_at DESC".to_string()
    };

    // LIMIT / OFFSET
    let limit_sql = match query.limit {
        Some(n) => format!(" LIMIT {n}"),
        None => String::new(),
    };
    let offset_sql = match query.offset {
        Some(n) => format!(" OFFSET {n}"),
        None => String::new(),
    };

    let sql = format!(
        "SELECT {select_sql} FROM {from_sql}{fts_join}{link_join}{where_sql}{order_sql}{limit_sql}{offset_sql}"
    );

    Ok(CompiledQuery {
        sql,
        params: ctx.params,
        uses_fts: ctx.uses_fts,
        uses_links: ctx.uses_links,
        uses_semantic: ctx.uses_semantic,
        near_params: ctx.near_params,
    })
}

struct CompileCtx {
    params: Vec<SqlParam>,
    uses_fts: bool,
    uses_links: bool,
    uses_semantic: bool,
    near_params: Option<(String, f64)>,
}

impl CompileCtx {
    fn new() -> Self {
        Self {
            params: Vec::new(),
            uses_fts: false,
            uses_links: false,
            uses_semantic: false,
            near_params: None,
        }
    }

    fn next_param(&mut self, param: SqlParam) -> usize {
        self.params.push(param);
        self.params.len()
    }

    fn next_param_for_type(&mut self, doc_type: &str) -> usize {
        self.next_param(SqlParam::Text(doc_type.to_string()))
    }
}

fn compile_select(select: &SelectClause) -> String {
    match select {
        SelectClause::Star => "d.*".to_string(),
        SelectClause::Fields(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|f| match &f.alias {
                    Some(alias) => format!("d.{} AS {alias}", f.name),
                    None => format!("d.{}", f.name),
                })
                .collect();
            parts.join(", ")
        }
    }
}

fn compile_where(wc: &WhereClause, ctx: &mut CompileCtx) -> Result<(String, bool), String> {
    match wc {
        WhereClause::Predicate(pred) => compile_predicate(pred, ctx),
        WhereClause::And(left, right) => {
            let (l, _) = compile_where(left, ctx)?;
            let (r, _) = compile_where(right, ctx)?;
            Ok((format!("({l} AND {r})"), false))
        }
        WhereClause::Or(left, right) => {
            let (l, _) = compile_where(left, ctx)?;
            let (r, _) = compile_where(right, ctx)?;
            Ok((format!("({l} OR {r})"), false))
        }
        WhereClause::Not(inner) => {
            let (sql, _) = compile_where(inner, ctx)?;
            Ok((format!("NOT ({sql})"), false))
        }
    }
}

fn compile_predicate(pred: &Predicate, ctx: &mut CompileCtx) -> Result<(String, bool), String> {
    match pred {
        Predicate::Comparison { field, op, value } => {
            let op_str = compile_comp_op(op);
            let idx = ctx.next_param(value_to_param(value));
            Ok((format!("d.{field} {op_str} ?{idx}"), false))
        }
        Predicate::InList { field, values } => {
            let placeholders: Vec<String> = values
                .iter()
                .map(|v| {
                    let idx = ctx.next_param(value_to_param(v));
                    format!("?{idx}")
                })
                .collect();
            Ok((format!("d.{field} IN ({})", placeholders.join(", ")), false))
        }
        Predicate::Like { field, pattern } => {
            let idx = ctx.next_param(SqlParam::Text(pattern.clone()));
            Ok((format!("d.{field} LIKE ?{idx}"), false))
        }
        Predicate::BodyContains { term } => {
            ctx.uses_fts = true;
            let idx = ctx.next_param(SqlParam::Text(term.clone()));
            Ok((format!("documents_fts MATCH ?{idx}"), true))
        }
        Predicate::Temporal(tf) => compile_temporal(tf, ctx),
        Predicate::Linked(lf) => compile_linked(lf, ctx),
        Predicate::Near { query, threshold } => {
            ctx.uses_semantic = true;
            ctx.near_params = Some((query.clone(), *threshold));
            // Placeholder: the executor will inject matching IDs
            // via a two-phase approach (KNN first, then filter by threshold,
            // then inject d.id IN (...) into the SQL)
            Ok(("1=1 /* NEAR placeholder */".to_string(), false))
        }
    }
}

fn compile_temporal(tf: &TemporalFunction, ctx: &mut CompileCtx) -> Result<(String, bool), String> {
    match tf {
        TemporalFunction::Fresh { duration } => {
            let cutoff = format!("-{duration}");
            let idx = ctx.next_param(SqlParam::Text(cutoff));
            Ok((format!("d.observed_at >= datetime('now', ?{idx})"), false))
        }
        TemporalFunction::Stale { duration } => {
            let cutoff = format!("-{duration}");
            let idx = ctx.next_param(SqlParam::Text(cutoff));
            Ok((format!("d.observed_at < datetime('now', ?{idx})"), false))
        }
        TemporalFunction::Expired => Ok(("d.valid_until < datetime('now')".to_string(), false)),
        TemporalFunction::Current => Ok((
            "(d.superseded_by IS NULL AND d.valid_until >= datetime('now'))".to_string(),
            false,
        )),
        TemporalFunction::Latest => {
            // Latest: not superseded
            Ok(("d.superseded_by IS NULL".to_string(), false))
        }
        TemporalFunction::AsOf { datetime } => {
            let idx = ctx.next_param(SqlParam::Text(datetime.clone()));
            Ok((
                format!(
                    "(d.observed_at <= ?{idx} AND d.valid_until >= ?{idx2})",
                    idx = idx,
                    idx2 = {
                        // Re-use the same datetime value as a second parameter
                        ctx.next_param(SqlParam::Text(datetime.clone()))
                    }
                ),
                false,
            ))
        }
        TemporalFunction::EffConfidence { op, threshold } => {
            let op_str = compile_comp_op(op);
            let idx = ctx.next_param(SqlParam::Float(*threshold));
            Ok((format!("d.confidence {op_str} ?{idx}"), false))
        }
    }
}

fn compile_linked(lf: &LinkedFunction, ctx: &mut CompileCtx) -> Result<(String, bool), String> {
    match lf {
        LinkedFunction::Forward { rel, target } => {
            let idx_rel = ctx.next_param(SqlParam::Text(rel.clone()));
            if let Some(t) = target {
                let idx_target = ctx.next_param(SqlParam::Text(t.clone()));
                Ok((
                    format!(
                        "d.id IN (SELECT source_id FROM links WHERE rel = ?{idx_rel} AND target_id = ?{idx_target})"
                    ),
                    false,
                ))
            } else {
                Ok((
                    format!("d.id IN (SELECT source_id FROM links WHERE rel = ?{idx_rel})"),
                    false,
                ))
            }
        }
        LinkedFunction::Reverse { rel, source } => {
            let idx_rel = ctx.next_param(SqlParam::Text(rel.clone()));
            if let Some(s) = source {
                let idx_source = ctx.next_param(SqlParam::Text(s.clone()));
                Ok((
                    format!(
                        "d.id IN (SELECT target_id FROM links WHERE rel = ?{idx_rel} AND source_id = ?{idx_source})"
                    ),
                    false,
                ))
            } else {
                Ok((
                    format!("d.id IN (SELECT target_id FROM links WHERE rel = ?{idx_rel})"),
                    false,
                ))
            }
        }
    }
}

fn compile_comp_op(op: &CompOp) -> &'static str {
    match op {
        CompOp::Eq => "=",
        CompOp::Neq => "!=",
        CompOp::Lt => "<",
        CompOp::Lte => "<=",
        CompOp::Gt => ">",
        CompOp::Gte => ">=",
    }
}

fn value_to_param(value: &Value) -> SqlParam {
    match value {
        Value::String(s) => SqlParam::Text(s.clone()),
        Value::Integer(i) => SqlParam::Integer(*i),
        Value::Float(f) => SqlParam::Float(*f),
        Value::Boolean(b) => SqlParam::Integer(i64::from(*b)),
        Value::Null => SqlParam::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkb_parser::parse_mkql;

    // === T-210.1: Field predicate compilation ===

    #[test]
    fn compile_equality_to_sql() {
        let query = parse_mkql("SELECT * FROM project WHERE status = 'active'").unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled.sql.contains("d.status = ?"));
        // Should have 2 params: doc_type + the value
        assert_eq!(compiled.params.len(), 2);
        assert!(matches!(&compiled.params[1], SqlParam::Text(s) if s == "active"));
    }

    #[test]
    fn compile_in_list_to_sql() {
        let query =
            parse_mkql("SELECT * FROM project WHERE status IN ('active', 'paused')").unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled.sql.contains("d.status IN ("));
        assert_eq!(compiled.params.len(), 3); // doc_type + 2 values
    }

    #[test]
    fn compile_body_contains_to_fts5() {
        let query =
            parse_mkql("SELECT * FROM meeting WHERE BODY CONTAINS 'machine learning'").unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled.uses_fts);
        assert!(compiled.sql.contains("documents_fts MATCH"));
        assert!(compiled.sql.contains("JOIN documents_fts"));
    }

    #[test]
    fn compile_parameterizes_values() {
        let query = parse_mkql("SELECT * FROM project WHERE status = 'active'").unwrap();
        let compiled = compile(&query).unwrap();
        // SQL should NOT contain literal 'active' — it should be parameterized
        assert!(!compiled.sql.contains("'active'"));
        assert!(compiled.sql.contains("?"));
    }

    // === T-210.2: Temporal function compilation ===

    #[test]
    fn compile_fresh_to_observed_at_range() {
        let query = parse_mkql("SELECT * FROM project WHERE FRESH('7d')").unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled.sql.contains("d.observed_at >= datetime('now'"));
    }

    #[test]
    fn compile_current_excludes_superseded_and_expired() {
        let query = parse_mkql("SELECT * FROM project WHERE CURRENT()").unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled.sql.contains("superseded_by IS NULL"));
        assert!(compiled.sql.contains("valid_until >= datetime('now')"));
    }

    #[test]
    fn compile_eff_confidence_with_decay() {
        let query = parse_mkql("SELECT * FROM project WHERE EFF_CONFIDENCE(> 0.5)").unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled.sql.contains("d.confidence >"));
    }

    // === T-210.3: Link clause compilation ===

    #[test]
    fn compile_forward_link_to_join() {
        let query = parse_mkql("SELECT * FROM project WHERE LINKED('owner')").unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled
            .sql
            .contains("SELECT source_id FROM links WHERE rel ="));
    }

    #[test]
    fn compile_reverse_link() {
        let query = parse_mkql("SELECT * FROM project WHERE LINKED(REVERSE, 'owner')").unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled
            .sql
            .contains("SELECT target_id FROM links WHERE rel ="));
    }

    #[test]
    fn compile_select_star_simple() {
        let query = parse_mkql("SELECT * FROM project").unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled.sql.starts_with("SELECT d.* FROM documents d"));
        assert!(compiled.sql.contains("d.doc_type = ?"));
        assert_eq!(compiled.params.len(), 1);
    }

    #[test]
    fn compile_select_specific_fields() {
        let query = parse_mkql("SELECT title, status FROM project").unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled.sql.contains("d.title, d.status"));
    }

    // === T-210.4: NEAR compilation ===

    #[test]
    fn compile_near_sets_semantic_flag() {
        let query =
            parse_mkql("SELECT * FROM project WHERE NEAR('machine learning', 0.8)").unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled.uses_semantic);
        assert!(compiled.near_params.is_some());
        let (q, t) = compiled.near_params.unwrap();
        assert_eq!(q, "machine learning");
        assert!((t - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn compile_near_combined_with_field() {
        let query = parse_mkql(
            "SELECT * FROM project WHERE NEAR('rust', 0.7) AND status = 'active'",
        )
        .unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled.uses_semantic);
        assert!(compiled.sql.contains("d.status ="));
    }

    #[test]
    fn compile_order_by_and_limit() {
        let query = parse_mkql("SELECT * FROM project ORDER BY observed_at DESC LIMIT 10").unwrap();
        let compiled = compile(&query).unwrap();
        assert!(compiled.sql.contains("ORDER BY d.observed_at DESC"));
        assert!(compiled.sql.contains("LIMIT 10"));
    }
}
