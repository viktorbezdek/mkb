//! # mkb-parser
//!
//! MKQL (Markdown Knowledge Query Language) parser using pest PEG grammar.
//!
//! Parses MKQL query strings into an AST that can be compiled to
//! SQLite SQL + vector index queries by the mkb-query crate.
//!
//! # Example
//!
//! ```
//! use mkb_parser::parse_mkql;
//! use mkb_parser::ast::{SelectClause, MkqlQuery};
//!
//! let query = parse_mkql("SELECT * FROM project").unwrap();
//! assert_eq!(query.from, "project");
//! assert_eq!(query.select, SelectClause::Star);
//! ```

pub mod ast;

use pest::Parser;
use pest_derive::Parser;

use ast::{
    CompOp, LinkedFunction, MkqlQuery, OrderByItem, Predicate, SelectClause, SelectField,
    SortDirection, TemporalFunction, Value, WhereClause,
};

#[derive(Parser)]
#[grammar = "mkql.pest"]
struct MkqlParser;

/// Parse error type for MKQL queries.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("MKQL parse error: {0}")]
    Grammar(String),
    #[error("unexpected rule: {0}")]
    UnexpectedRule(String),
}

/// Parse an MKQL query string into an AST.
///
/// # Errors
///
/// Returns [`ParseError`] if the query string is not valid MKQL.
pub fn parse_mkql(input: &str) -> Result<MkqlQuery, ParseError> {
    let pairs =
        MkqlParser::parse(Rule::query, input).map_err(|e| ParseError::Grammar(e.to_string()))?;

    let query_pair = pairs
        .into_iter()
        .next()
        .ok_or_else(|| ParseError::Grammar("empty parse result".to_string()))?;

    build_query(query_pair)
}

fn build_query(pair: pest::iterators::Pair<Rule>) -> Result<MkqlQuery, ParseError> {
    let mut select = SelectClause::Star;
    let mut from = String::new();
    let mut where_clause = None;
    let mut order_by = None;
    let mut limit = None;
    let mut offset = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::select_clause => {
                select = build_select_clause(inner)?;
            }
            Rule::from_clause => {
                from = build_from_clause(inner);
            }
            Rule::where_clause => {
                where_clause = Some(build_where_clause(inner)?);
            }
            Rule::order_by_clause => {
                order_by = Some(build_order_by(inner)?);
            }
            Rule::limit_clause => {
                limit = Some(build_limit(inner)?);
            }
            Rule::offset_clause => {
                offset = Some(build_offset(inner)?);
            }
            Rule::EOI => {}
            _ => {}
        }
    }

    Ok(MkqlQuery {
        select,
        from,
        where_clause,
        order_by,
        limit,
        offset,
    })
}

fn build_select_clause(pair: pest::iterators::Pair<Rule>) -> Result<SelectClause, ParseError> {
    let inner = pair
        .into_inner()
        .next()
        .ok_or_else(|| ParseError::UnexpectedRule("empty select clause".to_string()))?;

    match inner.as_rule() {
        Rule::star => Ok(SelectClause::Star),
        Rule::select_list => {
            let fields = inner
                .into_inner()
                .map(|f| build_select_field(f))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SelectClause::Fields(fields))
        }
        _ => Err(ParseError::UnexpectedRule(format!(
            "in select: {:?}",
            inner.as_rule()
        ))),
    }
}

fn build_select_field(pair: pest::iterators::Pair<Rule>) -> Result<SelectField, ParseError> {
    let mut inners = pair.into_inner();
    let name = inners
        .next()
        .ok_or_else(|| ParseError::UnexpectedRule("missing field name".to_string()))?
        .as_str()
        .to_string();
    let alias = inners.next().map(|a| a.as_str().to_string());
    Ok(SelectField { name, alias })
}

fn build_from_clause(pair: pest::iterators::Pair<Rule>) -> String {
    pair.into_inner()
        .next()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default()
}

fn build_where_clause(pair: pest::iterators::Pair<Rule>) -> Result<WhereClause, ParseError> {
    let or_expr = pair
        .into_inner()
        .next()
        .ok_or_else(|| ParseError::UnexpectedRule("empty where clause".to_string()))?;
    build_or_expr(or_expr)
}

fn build_or_expr(pair: pest::iterators::Pair<Rule>) -> Result<WhereClause, ParseError> {
    let mut inners = pair.into_inner();
    let first = inners
        .next()
        .ok_or_else(|| ParseError::UnexpectedRule("empty or_expr".to_string()))?;
    let mut result = build_and_expr(first)?;

    for next in inners {
        let right = build_and_expr(next)?;
        result = WhereClause::Or(Box::new(result), Box::new(right));
    }

    Ok(result)
}

fn build_and_expr(pair: pest::iterators::Pair<Rule>) -> Result<WhereClause, ParseError> {
    let mut inners = pair.into_inner();
    let first = inners
        .next()
        .ok_or_else(|| ParseError::UnexpectedRule("empty and_expr".to_string()))?;
    let mut result = build_not_expr(first)?;

    for next in inners {
        let right = build_not_expr(next)?;
        result = WhereClause::And(Box::new(result), Box::new(right));
    }

    Ok(result)
}

fn build_not_expr(pair: pest::iterators::Pair<Rule>) -> Result<WhereClause, ParseError> {
    let mut inners = pair.into_inner().peekable();
    let first = inners
        .peek()
        .ok_or_else(|| ParseError::UnexpectedRule("empty not_expr".to_string()))?;

    // Check if first child is an atom (no NOT keyword)
    if first.as_rule() == Rule::atom {
        let atom = inners.next().unwrap();
        build_atom(atom)
    } else {
        // NOT prefix — skip the first child (which is consumed by the grammar rule) and get the atom
        // In pest, `kw_not` is silent so we just get the atom
        let atom = inners.next().unwrap();
        let inner = build_atom(atom)?;
        Ok(WhereClause::Not(Box::new(inner)))
    }
}

fn build_atom(pair: pest::iterators::Pair<Rule>) -> Result<WhereClause, ParseError> {
    let inner = pair
        .into_inner()
        .next()
        .ok_or_else(|| ParseError::UnexpectedRule("empty atom".to_string()))?;

    match inner.as_rule() {
        Rule::comparison_pred => {
            let pred = build_comparison(inner)?;
            Ok(WhereClause::Predicate(pred))
        }
        Rule::in_pred => {
            let pred = build_in_pred(inner)?;
            Ok(WhereClause::Predicate(pred))
        }
        Rule::like_pred => {
            let pred = build_like_pred(inner)?;
            Ok(WhereClause::Predicate(pred))
        }
        Rule::body_contains_pred => {
            let pred = build_body_contains(inner)?;
            Ok(WhereClause::Predicate(pred))
        }
        Rule::temporal_fn => {
            let pred = build_temporal_fn(inner)?;
            Ok(WhereClause::Predicate(Predicate::Temporal(pred)))
        }
        Rule::linked_fn => {
            let pred = build_linked_fn(inner)?;
            Ok(WhereClause::Predicate(Predicate::Linked(pred)))
        }
        Rule::or_expr => build_or_expr(inner),
        _ => Err(ParseError::UnexpectedRule(format!(
            "in atom: {:?}",
            inner.as_rule()
        ))),
    }
}

fn build_comparison(pair: pest::iterators::Pair<Rule>) -> Result<Predicate, ParseError> {
    let mut inners = pair.into_inner();
    let field = inners.next().unwrap().as_str().to_string();
    let op = build_comp_op(inners.next().unwrap())?;
    let value = build_value(inners.next().unwrap())?;
    Ok(Predicate::Comparison { field, op, value })
}

fn build_comp_op(pair: pest::iterators::Pair<Rule>) -> Result<CompOp, ParseError> {
    match pair.as_str() {
        "=" => Ok(CompOp::Eq),
        "!=" => Ok(CompOp::Neq),
        "<" => Ok(CompOp::Lt),
        "<=" => Ok(CompOp::Lte),
        ">" => Ok(CompOp::Gt),
        ">=" => Ok(CompOp::Gte),
        other => Err(ParseError::UnexpectedRule(format!(
            "unknown operator: {other}"
        ))),
    }
}

fn build_value(pair: pest::iterators::Pair<Rule>) -> Result<Value, ParseError> {
    let inner = pair
        .into_inner()
        .next()
        .ok_or_else(|| ParseError::UnexpectedRule("empty value".to_string()))?;

    match inner.as_rule() {
        Rule::string_literal => {
            let s = inner.as_str();
            // Strip surrounding quotes
            Ok(Value::String(s[1..s.len() - 1].to_string()))
        }
        Rule::float_literal => {
            let f: f64 = inner
                .as_str()
                .parse()
                .map_err(|e: std::num::ParseFloatError| ParseError::Grammar(e.to_string()))?;
            Ok(Value::Float(f))
        }
        Rule::integer_literal => {
            let i: i64 = inner
                .as_str()
                .parse()
                .map_err(|e: std::num::ParseIntError| ParseError::Grammar(e.to_string()))?;
            Ok(Value::Integer(i))
        }
        Rule::boolean_literal => {
            let b = inner.as_str().eq_ignore_ascii_case("true");
            Ok(Value::Boolean(b))
        }
        Rule::null_literal => Ok(Value::Null),
        _ => Err(ParseError::UnexpectedRule(format!(
            "in value: {:?}",
            inner.as_rule()
        ))),
    }
}

fn build_in_pred(pair: pest::iterators::Pair<Rule>) -> Result<Predicate, ParseError> {
    let mut inners = pair.into_inner();
    let field = inners.next().unwrap().as_str().to_string();
    let in_list = inners.next().unwrap();
    let values = in_list
        .into_inner()
        .map(|v| build_value(v))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Predicate::InList { field, values })
}

fn build_like_pred(pair: pest::iterators::Pair<Rule>) -> Result<Predicate, ParseError> {
    let mut inners = pair.into_inner();
    let field = inners.next().unwrap().as_str().to_string();
    let pattern_raw = inners.next().unwrap().as_str();
    let pattern = pattern_raw[1..pattern_raw.len() - 1].to_string();
    Ok(Predicate::Like { field, pattern })
}

fn build_body_contains(pair: pest::iterators::Pair<Rule>) -> Result<Predicate, ParseError> {
    let inner = pair.into_inner().next().unwrap();
    let s = inner.as_str();
    let term = s[1..s.len() - 1].to_string();
    Ok(Predicate::BodyContains { term })
}

fn build_temporal_fn(pair: pest::iterators::Pair<Rule>) -> Result<TemporalFunction, ParseError> {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::fresh_fn => {
            let s = inner.into_inner().next().unwrap().as_str();
            let duration = s[1..s.len() - 1].to_string();
            Ok(TemporalFunction::Fresh { duration })
        }
        Rule::stale_fn => {
            let s = inner.into_inner().next().unwrap().as_str();
            let duration = s[1..s.len() - 1].to_string();
            Ok(TemporalFunction::Stale { duration })
        }
        Rule::expired_fn => Ok(TemporalFunction::Expired),
        Rule::current_fn => Ok(TemporalFunction::Current),
        Rule::latest_fn => Ok(TemporalFunction::Latest),
        Rule::as_of_fn => {
            let s = inner.into_inner().next().unwrap().as_str();
            let datetime = s[1..s.len() - 1].to_string();
            Ok(TemporalFunction::AsOf { datetime })
        }
        Rule::eff_conf_fn => {
            let mut inners = inner.into_inner();
            let op = build_comp_op(inners.next().unwrap())?;
            let threshold: f64 = inners
                .next()
                .unwrap()
                .as_str()
                .parse()
                .map_err(|e: std::num::ParseFloatError| ParseError::Grammar(e.to_string()))?;
            Ok(TemporalFunction::EffConfidence { op, threshold })
        }
        _ => Err(ParseError::UnexpectedRule(format!(
            "in temporal_fn: {:?}",
            inner.as_rule()
        ))),
    }
}

fn build_linked_fn(pair: pest::iterators::Pair<Rule>) -> Result<LinkedFunction, ParseError> {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::linked_forward => {
            let mut strings: Vec<String> = inner
                .into_inner()
                .map(|s| {
                    let raw = s.as_str();
                    raw[1..raw.len() - 1].to_string()
                })
                .collect();
            let rel = strings.remove(0);
            let target = if strings.is_empty() {
                None
            } else {
                Some(strings.remove(0))
            };
            Ok(LinkedFunction::Forward { rel, target })
        }
        Rule::linked_reverse => {
            let mut strings: Vec<String> = inner
                .into_inner()
                .map(|s| {
                    let raw = s.as_str();
                    raw[1..raw.len() - 1].to_string()
                })
                .collect();
            let rel = strings.remove(0);
            let source = if strings.is_empty() {
                None
            } else {
                Some(strings.remove(0))
            };
            Ok(LinkedFunction::Reverse { rel, source })
        }
        _ => Err(ParseError::UnexpectedRule(format!(
            "in linked_fn: {:?}",
            inner.as_rule()
        ))),
    }
}

fn build_order_by(pair: pest::iterators::Pair<Rule>) -> Result<Vec<OrderByItem>, ParseError> {
    pair.into_inner()
        .map(|item| {
            let mut inners = item.into_inner();
            let field = inners.next().unwrap().as_str().to_string();
            let direction = match inners.next() {
                Some(dir) => {
                    if dir.as_str().eq_ignore_ascii_case("desc") {
                        SortDirection::Desc
                    } else {
                        SortDirection::Asc
                    }
                }
                None => SortDirection::Asc,
            };
            Ok(OrderByItem { field, direction })
        })
        .collect()
}

fn build_limit(pair: pest::iterators::Pair<Rule>) -> Result<u64, ParseError> {
    let inner = pair.into_inner().next().unwrap();
    inner
        .as_str()
        .parse()
        .map_err(|e: std::num::ParseIntError| ParseError::Grammar(e.to_string()))
}

fn build_offset(pair: pest::iterators::Pair<Rule>) -> Result<u64, ParseError> {
    let inner = pair.into_inner().next().unwrap();
    inner
        .as_str()
        .parse()
        .map_err(|e: std::num::ParseIntError| ParseError::Grammar(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;

    // === T-200.1: SELECT statements ===

    #[test]
    fn parse_select_star_from_type() {
        let q = parse_mkql("SELECT * FROM project").unwrap();
        assert_eq!(q.select, SelectClause::Star);
        assert_eq!(q.from, "project");
        assert!(q.where_clause.is_none());
    }

    #[test]
    fn parse_select_specific_fields() {
        let q = parse_mkql("SELECT title, status FROM project").unwrap();
        match &q.select {
            SelectClause::Fields(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "title");
                assert_eq!(fields[1].name, "status");
            }
            _ => panic!("expected Fields"),
        }
    }

    #[test]
    fn parse_select_with_alias() {
        let q = parse_mkql("SELECT title AS t, status AS s FROM project").unwrap();
        match &q.select {
            SelectClause::Fields(fields) => {
                assert_eq!(fields[0].alias, Some("t".to_string()));
                assert_eq!(fields[1].alias, Some("s".to_string()));
            }
            _ => panic!("expected Fields"),
        }
    }

    // === T-200.2: WHERE clauses ===

    #[test]
    fn parse_equality_predicate() {
        let q = parse_mkql("SELECT * FROM project WHERE status = 'active'").unwrap();
        match &q.where_clause {
            Some(WhereClause::Predicate(Predicate::Comparison { field, op, value })) => {
                assert_eq!(field, "status");
                assert_eq!(*op, CompOp::Eq);
                assert_eq!(*value, Value::String("active".to_string()));
            }
            other => panic!("expected comparison, got {other:?}"),
        }
    }

    #[test]
    fn parse_comparison_operators() {
        let q = parse_mkql("SELECT * FROM project WHERE confidence > 0.5").unwrap();
        match &q.where_clause {
            Some(WhereClause::Predicate(Predicate::Comparison { op, value, .. })) => {
                assert_eq!(*op, CompOp::Gt);
                assert_eq!(*value, Value::Float(0.5));
            }
            other => panic!("expected comparison, got {other:?}"),
        }

        let q = parse_mkql("SELECT * FROM project WHERE count <= 100").unwrap();
        match &q.where_clause {
            Some(WhereClause::Predicate(Predicate::Comparison { op, value, .. })) => {
                assert_eq!(*op, CompOp::Lte);
                assert_eq!(*value, Value::Integer(100));
            }
            other => panic!("expected comparison, got {other:?}"),
        }
    }

    #[test]
    fn parse_in_list() {
        let q = parse_mkql("SELECT * FROM project WHERE status IN ('active', 'paused')").unwrap();
        match &q.where_clause {
            Some(WhereClause::Predicate(Predicate::InList { field, values })) => {
                assert_eq!(field, "status");
                assert_eq!(values.len(), 2);
                assert_eq!(values[0], Value::String("active".to_string()));
                assert_eq!(values[1], Value::String("paused".to_string()));
            }
            other => panic!("expected in_list, got {other:?}"),
        }
    }

    #[test]
    fn parse_like_pattern() {
        let q = parse_mkql("SELECT * FROM project WHERE title LIKE 'Alpha%'").unwrap();
        match &q.where_clause {
            Some(WhereClause::Predicate(Predicate::Like { field, pattern })) => {
                assert_eq!(field, "title");
                assert_eq!(pattern, "Alpha%");
            }
            other => panic!("expected like, got {other:?}"),
        }
    }

    #[test]
    fn parse_and_or_combinations() {
        let q = parse_mkql("SELECT * FROM project WHERE status = 'active' AND confidence > 0.5")
            .unwrap();
        assert!(matches!(q.where_clause, Some(WhereClause::And(_, _))));

        let q = parse_mkql("SELECT * FROM project WHERE status = 'active' OR status = 'paused'")
            .unwrap();
        assert!(matches!(q.where_clause, Some(WhereClause::Or(_, _))));
    }

    #[test]
    fn parse_body_contains() {
        let q = parse_mkql("SELECT * FROM meeting WHERE BODY CONTAINS 'machine learning'").unwrap();
        match &q.where_clause {
            Some(WhereClause::Predicate(Predicate::BodyContains { term })) => {
                assert_eq!(term, "machine learning");
            }
            other => panic!("expected body_contains, got {other:?}"),
        }
    }

    // === T-200.3: Temporal functions ===

    #[test]
    fn parse_fresh_duration() {
        let q = parse_mkql("SELECT * FROM project WHERE FRESH('7d')").unwrap();
        match &q.where_clause {
            Some(WhereClause::Predicate(Predicate::Temporal(TemporalFunction::Fresh {
                duration,
            }))) => {
                assert_eq!(duration, "7d");
            }
            other => panic!("expected fresh, got {other:?}"),
        }
    }

    #[test]
    fn parse_stale_and_expired() {
        let q = parse_mkql("SELECT * FROM project WHERE STALE('30d')").unwrap();
        assert!(matches!(
            &q.where_clause,
            Some(WhereClause::Predicate(Predicate::Temporal(
                TemporalFunction::Stale { .. }
            )))
        ));

        let q = parse_mkql("SELECT * FROM project WHERE EXPIRED()").unwrap();
        assert!(matches!(
            &q.where_clause,
            Some(WhereClause::Predicate(Predicate::Temporal(
                TemporalFunction::Expired
            )))
        ));
    }

    #[test]
    fn parse_current_and_latest() {
        let q = parse_mkql("SELECT * FROM project WHERE CURRENT()").unwrap();
        assert!(matches!(
            &q.where_clause,
            Some(WhereClause::Predicate(Predicate::Temporal(
                TemporalFunction::Current
            )))
        ));

        let q = parse_mkql("SELECT * FROM project WHERE LATEST()").unwrap();
        assert!(matches!(
            &q.where_clause,
            Some(WhereClause::Predicate(Predicate::Temporal(
                TemporalFunction::Latest
            )))
        ));
    }

    #[test]
    fn parse_as_of_datetime() {
        let q = parse_mkql("SELECT * FROM project WHERE AS_OF('2025-02-10T00:00:00Z')").unwrap();
        match &q.where_clause {
            Some(WhereClause::Predicate(Predicate::Temporal(TemporalFunction::AsOf {
                datetime,
            }))) => {
                assert_eq!(datetime, "2025-02-10T00:00:00Z");
            }
            other => panic!("expected as_of, got {other:?}"),
        }
    }

    #[test]
    fn parse_eff_confidence() {
        let q = parse_mkql("SELECT * FROM project WHERE EFF_CONFIDENCE(> 0.5)").unwrap();
        match &q.where_clause {
            Some(WhereClause::Predicate(Predicate::Temporal(
                TemporalFunction::EffConfidence { op, threshold },
            ))) => {
                assert_eq!(*op, CompOp::Gt);
                assert!((threshold - 0.5).abs() < f64::EPSILON);
            }
            other => panic!("expected eff_confidence, got {other:?}"),
        }
    }

    // === T-200.4: LINKED function ===

    #[test]
    fn parse_linked_forward() {
        let q = parse_mkql("SELECT * FROM project WHERE LINKED('owner')").unwrap();
        match &q.where_clause {
            Some(WhereClause::Predicate(Predicate::Linked(LinkedFunction::Forward {
                rel,
                target,
            }))) => {
                assert_eq!(rel, "owner");
                assert!(target.is_none());
            }
            other => panic!("expected linked forward, got {other:?}"),
        }
    }

    #[test]
    fn parse_linked_reverse() {
        let q = parse_mkql("SELECT * FROM project WHERE LINKED(REVERSE, 'owner')").unwrap();
        match &q.where_clause {
            Some(WhereClause::Predicate(Predicate::Linked(LinkedFunction::Reverse {
                rel,
                source,
            }))) => {
                assert_eq!(rel, "owner");
                assert!(source.is_none());
            }
            other => panic!("expected linked reverse, got {other:?}"),
        }
    }

    #[test]
    fn parse_linked_with_filter() {
        let q =
            parse_mkql("SELECT * FROM project WHERE LINKED('owner', 'people/jane-smith')").unwrap();
        match &q.where_clause {
            Some(WhereClause::Predicate(Predicate::Linked(LinkedFunction::Forward {
                rel,
                target,
            }))) => {
                assert_eq!(rel, "owner");
                assert_eq!(*target, Some("people/jane-smith".to_string()));
            }
            other => panic!("expected linked with target, got {other:?}"),
        }
    }

    // === T-200.5: ORDER BY, LIMIT, OFFSET ===

    #[test]
    fn parse_order_by_multiple_fields() {
        let q = parse_mkql("SELECT * FROM project ORDER BY observed_at DESC, title ASC").unwrap();
        let order = q.order_by.unwrap();
        assert_eq!(order.len(), 2);
        assert_eq!(order[0].field, "observed_at");
        assert_eq!(order[0].direction, SortDirection::Desc);
        assert_eq!(order[1].field, "title");
        assert_eq!(order[1].direction, SortDirection::Asc);
    }

    #[test]
    fn parse_limit_and_offset() {
        let q = parse_mkql("SELECT * FROM project LIMIT 10 OFFSET 20").unwrap();
        assert_eq!(q.limit, Some(10));
        assert_eq!(q.offset, Some(20));
    }

    // === T-200.6: Parser error messages ===

    #[test]
    fn parser_error_messages_are_helpful() {
        let result = parse_mkql("INVALID QUERY");
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("parse error"),
            "Error should mention parse: {msg}"
        );
    }

    // === Complex combined queries ===

    #[test]
    fn parse_full_complex_query() {
        let q = parse_mkql(
            "SELECT title, status FROM project WHERE status = 'active' AND CURRENT() ORDER BY observed_at DESC LIMIT 10",
        )
        .unwrap();

        match &q.select {
            SelectClause::Fields(fields) => assert_eq!(fields.len(), 2),
            _ => panic!("expected Fields"),
        }
        assert_eq!(q.from, "project");
        assert!(matches!(q.where_clause, Some(WhereClause::And(_, _))));
        assert_eq!(q.order_by.unwrap().len(), 1);
        assert_eq!(q.limit, Some(10));
    }

    #[test]
    fn parse_case_insensitive_keywords() {
        let q = parse_mkql("select * from project where status = 'active'").unwrap();
        assert_eq!(q.from, "project");
        assert!(q.where_clause.is_some());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Generate valid MKQL query strings.
    fn valid_query_strategy() -> impl Strategy<Value = String> {
        let doc_types =
            prop::sample::select(vec!["project", "meeting", "decision", "signal", "person"]);
        let fields = prop::sample::select(vec![
            "title",
            "status",
            "confidence",
            "observed_at",
            "source",
        ]);
        let where_clauses = prop::sample::select(vec![
            "".to_string(),
            " WHERE CURRENT()".to_string(),
            " WHERE EXPIRED()".to_string(),
            " WHERE LATEST()".to_string(),
            " WHERE FRESH('7d')".to_string(),
            " WHERE status = 'active'".to_string(),
            " WHERE confidence > 0.5".to_string(),
        ]);

        (doc_types, fields, where_clauses).prop_map(|(doc_type, _field, where_clause)| {
            format!("SELECT * FROM {doc_type}{where_clause}")
        })
    }

    proptest! {
        #[test]
        fn valid_queries_parse(query in valid_query_strategy()) {
            let result = parse_mkql(&query);
            prop_assert!(result.is_ok(), "Valid query should parse: {query}, error: {:?}", result.err());
        }

        #[test]
        fn random_strings_dont_panic(s in "\\PC{0,100}") {
            // Should never panic — may return Ok or Err but must not crash
            let _ = parse_mkql(&s);
        }
    }
}
