//! The Readactus pipeline: connect → reflect → detect → plan → copy.
//!
//! Every stage operates against DDBCore's engine-agnostic `Connection`
//! trait, so the pipeline works identically for any engine with an
//! adapter. Two invariants are enforced HERE, not left to callers:
//!
//! 1. Source connections are always opened read-only — Readactus never
//!    modifies a production database, period.
//! 2. Transformation is deterministic within a run, so every occurrence
//!    of a value transforms identically across all tables and referential
//!    integrity survives.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use ddbcore::{
    Catalog, Connection, ConnectionConfig, DatabaseAdapter, DdbCoreError, Row, RowStream,
    StreamOptions, TableRef, Value,
};
use futures::StreamExt;
use readactus_detect::{Detector, Finding, PiiKind, RuleBasedDetector};
use readactus_license::Entitlements;
use readactus_transform::Tokenizer;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReadactusError {
    #[error(transparent)]
    Database(#[from] DdbCoreError),
    #[error("planning error: {0}")]
    Plan(String),
    #[error(
        "extraction limit reached: this copy exceeds the {limit} byte free-tier cap. \
         The run has been aborted and the partial copy is NOT resumable. \
         Register Readactus to remove the limit."
    )]
    ExtractionCapExceeded { limit: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Engine {
    Postgres,
    MySql,
}

impl std::str::FromStr for Engine {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" | "pg" => Ok(Engine::Postgres),
            "mysql" | "mariadb" => Ok(Engine::MySql),
            other => Err(format!("unknown engine '{other}' (expected postgres|mysql)")),
        }
    }
}

/// Opens a SOURCE connection: read-only is forced on regardless of what
/// the caller passed. Principle: production databases are never modified.
pub async fn connect_source(engine: Engine, mut config: ConnectionConfig) -> Result<Box<dyn Connection>, ReadactusError> {
    config.read_only = true;
    connect(engine, config).await
}

/// Opens a TARGET connection (writable — it receives the transformed copy).
pub async fn connect_target(engine: Engine, mut config: ConnectionConfig) -> Result<Box<dyn Connection>, ReadactusError> {
    config.read_only = false;
    connect(engine, config).await
}

async fn connect(engine: Engine, config: ConnectionConfig) -> Result<Box<dyn Connection>, ReadactusError> {
    let conn = match engine {
        Engine::Postgres => ddbcore_postgres::PostgresAdapter.connect(&config).await?,
        Engine::MySql => ddbcore_mysql::MySqlAdapter.connect(&config).await?,
    };
    Ok(conn)
}

/// What to do with one column during the copy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColumnAction {
    /// Copy verbatim.
    Passthrough,
    /// Replace via deterministic tokenization as the given kind.
    Tokenize(PiiKind),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnPlan {
    pub column: String,
    pub action: ColumnAction,
    /// The detection finding this action came from, if any — kept so the
    /// UI can show WHY a column is being transformed and let the user
    /// override it.
    pub finding: Option<Finding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TablePlan {
    pub table: TableRef,
    pub columns: Vec<ColumnPlan>,
}

/// The full transformation plan: one entry per table, one per column, in
/// table-column order. Automatically derived from detection, and fully
/// editable before execution — automation recommends, the human decides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub tables: Vec<TablePlan>,
}

/// Runs detection across the catalog and derives a plan: findings at or
/// above `confidence_threshold` become `Tokenize` actions, everything
/// else passes through.
pub fn build_plan(catalog: &Catalog, confidence_threshold: f32) -> (Plan, Vec<Finding>) {
    let detector = RuleBasedDetector;
    let mut all_findings = Vec::new();
    let mut tables = Vec::new();

    for schema in &catalog.schemas {
        for table in &schema.tables {
            let findings = detector.detect_table(table);
            let by_column: HashMap<&str, &Finding> =
                findings.iter().map(|f| (f.column.as_str(), f)).collect();

            let columns = table
                .columns
                .iter()
                .map(|col| {
                    let finding = by_column.get(col.name.as_str()).copied().cloned();
                    let action = match &finding {
                        Some(f) if f.confidence >= confidence_threshold => ColumnAction::Tokenize(f.kind.clone()),
                        _ => ColumnAction::Passthrough,
                    };
                    ColumnPlan { column: col.name.clone(), action, finding }
                })
                .collect();

            tables.push(TablePlan { table: table.table_ref(), columns });
            all_findings.extend(findings);
        }
    }

    (Plan { tables }, all_findings)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableReport {
    pub table: TableRef,
    pub rows_copied: u64,
    pub columns_transformed: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CopyReport {
    pub tables: Vec<TableReport>,
    pub total_rows: u64,
}

/// Rough wire-size estimate of one row, mirroring the accounting used
/// for batch budgeting in the adapters: variable-size payloads at actual
/// length, fixed overhead for scalars.
fn estimated_row_bytes(row: &Row) -> u64 {
    row.0
        .iter()
        .map(|v| match v {
            Value::Text(s) => s.len() as u64 + 16,
            Value::Binary(b) => b.len() as u64 + 16,
            Value::Json(j) => match j {
                serde_json::Value::String(s) => s.len() as u64 + 32,
                _ => 256,
            },
            Value::Array(items) => 16 + items.len() as u64 * 16,
            _ => 16,
        })
        .sum()
}

/// Wraps a source row stream with hard byte metering against a cap shared
/// across every table in the run. The moment the next row would cross the
/// cap, the stream yields an error and the run dies — deliberately no
/// truncate-and-continue and no resume: a capped run produces a partial
/// copy and says so.
fn meter_stream(rows: RowStream, used: Arc<AtomicU64>, cap: u64) -> RowStream {
    let metered = rows.map(move |row| {
        let row = row?;
        let bytes = estimated_row_bytes(&row);
        let prior = used.fetch_add(bytes, Ordering::SeqCst);
        if prior + bytes > cap {
            return Err(DdbCoreError::Query(format!(
                "readactus free-tier extraction cap of {cap} bytes exceeded"
            )));
        }
        Ok(row)
    });
    Box::pin(metered)
}

/// Executes the copy: recreate structure on the target, then stream every
/// table through the tokenizer.
///
/// Structure is applied in two phases: all `CREATE TABLE` statements
/// first and constraints/indexes AFTER data load — FKs can't fail on
/// load order, and index maintenance doesn't slow the bulk write.
///
/// `entitlements` gates extraction volume: on the free tier the TOTAL
/// bytes streamed from the source across all tables is hard-capped, and
/// crossing the cap aborts the run mid-stream, unresumable.
pub async fn run_copy(
    source: &dyn Connection,
    target: &dyn Connection,
    catalog: &Catalog,
    plan: &Plan,
    tokenizer: Arc<Tokenizer>,
    entitlements: &Entitlements,
) -> Result<CopyReport, ReadactusError> {
    let plans_by_table: HashMap<&TableRef, &TablePlan> =
        plan.tables.iter().map(|tp| (&tp.table, tp)).collect();

    let mut deferred: Vec<String> = Vec::new();

    // Phase 1: bare tables.
    for schema in &catalog.schemas {
        for table in &schema.tables {
            let statements = target.render_ddl(table)?;
            for statement in statements {
                if statement.trim_start().starts_with("CREATE TABLE") {
                    target.execute_query(&statement, &[]).await?;
                } else {
                    deferred.push(statement);
                }
            }
        }
    }

    // Phase 2: data, transformed. One shared byte counter across all
    // tables — the cap is per run, not per table.
    let bytes_used = Arc::new(AtomicU64::new(0));
    let mut report = CopyReport::default();
    for schema in &catalog.schemas {
        for table in &schema.tables {
            let table_ref = table.table_ref();
            let table_plan = plans_by_table.get(&table_ref);

            // Column index → tokenize action, in reflected column order
            // (the same order stream_rows yields).
            let actions: Vec<Option<&PiiKind>> = table
                .columns
                .iter()
                .map(|col| {
                    table_plan.and_then(|tp| {
                        tp.columns.iter().find(|cp| cp.column == col.name).and_then(|cp| match &cp.action {
                            ColumnAction::Tokenize(kind) => Some(kind),
                            ColumnAction::Passthrough => None,
                        })
                    })
                })
                .collect();
            let transformed_count = actions.iter().filter(|a| a.is_some()).count();

            let rows = source.stream_rows(&table_ref, StreamOptions::default()).await?;
            // Metering wraps the RAW source stream — the cap measures
            // what is extracted from production, before transformation.
            let rows = match entitlements.max_extract_bytes {
                Some(cap) => meter_stream(rows, Arc::clone(&bytes_used), cap),
                None => rows,
            };
            let actions_owned: Vec<Option<PiiKind>> = actions.iter().map(|a| a.cloned()).collect();
            let tokenizer = Arc::clone(&tokenizer);

            let transformed = rows.map(move |row| {
                row.map(|mut row| {
                    for (i, action) in actions_owned.iter().enumerate() {
                        if let (Some(kind), Some(value)) = (action, row.0.get_mut(i)) {
                            *value = tokenizer.transform(kind, value);
                        }
                    }
                    row
                })
            });

            let rows_copied = target.bulk_write(&table_ref, Box::pin(transformed)).await.map_err(|e| {
                // Surface the cap abort as its own error type so callers
                // can distinguish "you hit the free-tier wall" from a
                // genuine database failure.
                if let Some(cap) = entitlements.max_extract_bytes {
                    if e.to_string().contains("extraction cap") {
                        return ReadactusError::ExtractionCapExceeded { limit: cap };
                    }
                }
                ReadactusError::Database(e)
            })?;
            report.total_rows += rows_copied;
            report.tables.push(TableReport { table: table_ref, rows_copied, columns_transformed: transformed_count });
        }
    }

    // Phase 3: constraints and indexes.
    for statement in &deferred {
        target.execute_query(statement, &[]).await?;
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;

    fn row_of_text(len: usize) -> Row {
        Row(vec![Value::Integer(1), Value::Text("x".repeat(len))])
    }

    fn stream_of(rows: Vec<Row>) -> RowStream {
        Box::pin(stream::iter(rows.into_iter().map(Ok)))
    }

    #[tokio::test]
    async fn meter_allows_rows_under_cap() {
        // 3 rows × (100 text + 16 + 16 int overhead) = 396 bytes < 1000.
        let rows = stream_of(vec![row_of_text(100), row_of_text(100), row_of_text(100)]);
        let used = Arc::new(AtomicU64::new(0));
        let mut metered = meter_stream(rows, Arc::clone(&used), 1000);
        let mut count = 0;
        while let Some(row) = metered.next().await {
            row.expect("under-cap rows must pass");
            count += 1;
        }
        assert_eq!(count, 3);
        assert!(used.load(Ordering::SeqCst) <= 1000);
    }

    #[tokio::test]
    async fn meter_kills_stream_the_moment_cap_would_be_crossed() {
        // Row ~132 bytes; cap 300: rows 1-2 pass (264), row 3 must die.
        let rows = stream_of(vec![row_of_text(100), row_of_text(100), row_of_text(100), row_of_text(100)]);
        let mut metered = meter_stream(rows, Arc::new(AtomicU64::new(0)), 300);

        assert!(metered.next().await.unwrap().is_ok());
        assert!(metered.next().await.unwrap().is_ok());
        let third = metered.next().await.unwrap();
        assert!(third.is_err(), "third row must abort the stream");
        let msg = third.unwrap_err().to_string();
        assert!(msg.contains("extraction cap"), "error must identify the cap, got: {msg}");
    }

    #[tokio::test]
    async fn meter_cap_is_shared_across_streams() {
        // Two tables sharing one counter: the second stream inherits the
        // first's usage — the cap is per RUN, not per table.
        let used = Arc::new(AtomicU64::new(0));
        let mut first = meter_stream(stream_of(vec![row_of_text(100), row_of_text(100)]), Arc::clone(&used), 300);
        while let Some(row) = first.next().await {
            row.expect("first table fits");
        }

        let mut second = meter_stream(stream_of(vec![row_of_text(100)]), Arc::clone(&used), 300);
        let row = second.next().await.unwrap();
        assert!(row.is_err(), "second table must inherit the run-level usage and die");
    }

    #[tokio::test]
    async fn single_oversized_row_dies_immediately() {
        // "Absolutely no larger": even the FIRST row aborts if it alone
        // would cross the cap.
        let mut metered = meter_stream(stream_of(vec![row_of_text(5000)]), Arc::new(AtomicU64::new(0)), 300);
        assert!(metered.next().await.unwrap().is_err());
    }
}
