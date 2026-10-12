//! Local analytical queries over a project's datasets, with DuckDB.
//!
//! Files are queried where they are (`read_csv`, `read_parquet`,
//! `read_json`), so a 10 GB Parquet file is never loaded whole: previews
//! fetch one page, statistics run inside DuckDB, and only the result
//! crosses to the UI.
//!
//! Every connection is locked down when the engine opens: DuckDB may only
//! read files under the project root, may not install or download
//! extensions, and its configuration is then locked so a query can't undo
//! any of that. Each query runs under a memory limit, a row cap, a timeout,
//! and can be cancelled from another thread.

mod values;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use duckdb::{params_from_iter, Connection};
use serde::{Deserialize, Serialize};

use epoch_core::fs::FileKind;
use epoch_core::ProjectRoot;

pub use values::to_json;

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("{0}")]
    Query(String),
    #[error("the query took longer than {0:?} and was stopped")]
    Timeout(Duration),
    #[error("the query was cancelled")]
    Cancelled,
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Core(#[from] epoch_core::CoreError),
}

impl From<duckdb::Error> for DataError {
    fn from(e: duckdb::Error) -> Self {
        Self::Query(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, DataError>;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub memory_limit_mb: u32,
    /// 0 lets DuckDB decide.
    pub threads: u32,
    pub timeout: Duration,
    /// Most rows a single request may return.
    pub max_rows: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_limit_mb: 2_048,
            threads: 0,
            timeout: Duration::from_secs(60),
            max_rows: 10_000,
        }
    }
}

/// Cancels the query it was handed out for. Cheap to clone and `Send`.
#[derive(Clone)]
pub struct CancelHandle {
    interrupt: Arc<duckdb::InterruptHandle>,
    cancelled: Arc<AtomicBool>,
}

impl CancelHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.interrupt.interrupt();
    }
}

/// A DuckDB database scoped to one project.
pub struct DataEngine {
    /// Held only while cloning; each request runs on its own clone.
    base: std::sync::Mutex<Connection>,
    root: PathBuf,
    limits: Limits,
}

/// A SQL string literal.
fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// A SQL identifier.
fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

impl DataEngine {
    pub fn open(root: &ProjectRoot, limits: Limits) -> Result<Self> {
        let base = Connection::open_in_memory()?;
        let root_path = root.path().to_path_buf();
        let mut setup = vec![
            format!("SET memory_limit = '{}MB'", limits.memory_limit_mb),
            "SET autoinstall_known_extensions = false".to_string(),
            "SET autoload_known_extensions = false".to_string(),
        ];
        if limits.threads > 0 {
            setup.push(format!("SET threads = {}", limits.threads));
        }
        // The order matters: allow the project folder, then switch external
        // access off (which keeps only the allowed folders readable), then
        // lock the configuration so SQL can't SET it back.
        let mut dir = root_path.display().to_string();
        if !dir.ends_with(std::path::MAIN_SEPARATOR) {
            dir.push(std::path::MAIN_SEPARATOR);
        }
        setup.push(format!("SET allowed_directories = [{}]", literal(&dir)));
        setup.push("SET enable_external_access = false".to_string());
        setup.push("SET lock_configuration = true".to_string());
        base.execute_batch(&setup.join(";\n"))?;
        Ok(Self {
            base: std::sync::Mutex::new(base),
            root: root_path,
            limits,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Make a project dataset queryable by name in the SQL editor: creates
    /// (or replaces) a view over the file and returns the view's name. DuckDB
    /// checks file access against the raw path text, so relative paths in
    /// ad-hoc SQL are refused by the sandbox; views carry the absolute path.
    pub fn register_view(&self, rel: &str, name: &str) -> Result<String> {
        let (src, _, _) = self.source(rel)?;
        let name: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        let name = if name.starts_with(|c: char| c.is_ascii_alphabetic()) {
            name
        } else {
            format!("t_{name}")
        };
        let conn = self.connection()?;
        conn.execute_batch(&format!(
            "CREATE OR REPLACE VIEW {} AS SELECT * FROM {src}",
            ident(&name)
        ))?;
        Ok(name)
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// A connection to the same database, for one request on one thread.
    fn connection(&self) -> Result<Connection> {
        let base = self.base.lock().unwrap_or_else(|e| e.into_inner());
        Ok(base.try_clone()?)
    }

    /// Run `f` on a fresh connection, interrupting it at the timeout or when
    /// the handle given to `on_start` is cancelled.
    fn guarded<T>(
        &self,
        on_start: impl FnOnce(CancelHandle),
        f: impl FnOnce(&Connection) -> Result<T>,
    ) -> Result<T> {
        let conn = self.connection()?;
        let handle = CancelHandle {
            interrupt: conn.interrupt_handle(),
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        on_start(handle.clone());
        let timed_out = Arc::new(AtomicBool::new(false));
        let (done, wait) = mpsc::channel::<()>();
        let watchdog = {
            let interrupt = handle.interrupt.clone();
            let timed_out = timed_out.clone();
            let timeout = self.limits.timeout;
            std::thread::spawn(move || {
                if let Err(mpsc::RecvTimeoutError::Timeout) = wait.recv_timeout(timeout) {
                    timed_out.store(true, Ordering::SeqCst);
                    interrupt.interrupt();
                }
            })
        };
        let result = f(&conn);
        let _ = done.send(());
        let _ = watchdog.join();
        match result {
            Err(_) if timed_out.load(Ordering::SeqCst) => {
                Err(DataError::Timeout(self.limits.timeout))
            }
            Err(_) if handle.cancelled.load(Ordering::SeqCst) => Err(DataError::Cancelled),
            other => other,
        }
    }

    /// A table function reading `rel` (relative to the project root).
    fn source(&self, rel: &str) -> Result<(String, FileKind, PathBuf)> {
        let root = ProjectRoot::open(&self.root)?;
        let path = root.resolve(rel)?;
        if !path.is_file() {
            return Err(epoch_core::CoreError::NotFound { path }.into());
        }
        let kind = FileKind::from_path(&path);
        let lit = literal(&path.display().to_string());
        let is_tsv = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("tsv"));
        let sql = match kind {
            FileKind::Csv if is_tsv => {
                format!("read_csv({lit}, delim = '\\t', sample_size = 20480)")
            }
            FileKind::Csv => format!("read_csv({lit}, sample_size = 20480)"),
            FileKind::Parquet => format!("read_parquet({lit})"),
            FileKind::Json => format!("read_json({lit})"),
            FileKind::Jsonl => format!("read_json({lit}, format = 'newline_delimited')"),
            _ => {
                return Err(DataError::Invalid(format!(
                    "{rel}: not a CSV, Parquet or JSON file"
                )))
            }
        };
        Ok((sql, kind, path))
    }

    /// Column names and types, and the row count.
    pub fn describe(&self, rel: &str, on_start: impl FnOnce(CancelHandle)) -> Result<DatasetInfo> {
        let (src, kind, path) = self.source(rel)?;
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        self.guarded(on_start, |conn| {
            let mut stmt = conn.prepare(&format!("DESCRIBE SELECT * FROM {src}"))?;
            let columns = stmt
                .query_map([], |r| {
                    Ok(Column {
                        name: r.get(0)?,
                        data_type: r.get(1)?,
                    })
                })?
                .collect::<duckdb::Result<Vec<_>>>()?;
            let rows: i64 =
                conn.query_row(&format!("SELECT count(*) FROM {src}"), [], |r| r.get(0))?;
            Ok(DatasetInfo {
                path: rel.to_string(),
                kind,
                size,
                columns,
                row_count: rows as u64,
            })
        })
    }

    /// One page of rows, sorted and filtered.
    pub fn page(
        &self,
        rel: &str,
        req: &PageRequest,
        on_start: impl FnOnce(CancelHandle),
    ) -> Result<Page> {
        let (src, _, _) = self.source(rel)?;
        let limit = req.limit.clamp(1, self.limits.max_rows);
        self.guarded(on_start, |conn| {
            let columns = describe_columns(conn, &src)?;
            let known = |name: &str| columns.iter().any(|c| c.name == name);
            let (where_sql, params) = filters_sql(&req.filters, &known)?;
            let mut order = Vec::new();
            for s in &req.sort {
                if !known(&s.column) {
                    return Err(DataError::Invalid(format!("no column named {}", s.column)));
                }
                order.push(format!(
                    "{} {} NULLS LAST",
                    ident(&s.column),
                    if s.desc { "DESC" } else { "ASC" }
                ));
            }
            let order_sql = if order.is_empty() {
                String::new()
            } else {
                format!(" ORDER BY {}", order.join(", "))
            };
            let sql = format!(
                "SELECT * FROM {src}{where_sql}{order_sql} LIMIT {limit} OFFSET {}",
                req.offset
            );
            let rows = collect_rows(conn, &sql, &params, limit)?.0;
            let total = if req.with_total {
                let count_sql = format!("SELECT count(*) FROM {src}{where_sql}");
                let mut stmt = conn.prepare(&count_sql)?;
                let n: i64 = stmt.query_row(params_from_iter(params.iter()), |r| r.get(0))?;
                Some(n as u64)
            } else {
                None
            };
            Ok(Page {
                offset: req.offset,
                rows,
                total,
                columns,
            })
        })
    }

    /// DuckDB's `SUMMARIZE`: per-column min, max, distinct estimate, mean,
    /// quartiles and null share.
    pub fn stats(&self, rel: &str, on_start: impl FnOnce(CancelHandle)) -> Result<QueryResult> {
        let (src, _, _) = self.source(rel)?;
        let started = Instant::now();
        self.guarded(on_start, |conn| {
            let (rows, columns, truncated) =
                collect_rows(conn, &format!("SUMMARIZE SELECT * FROM {src}"), &[], 10_000)?;
            Ok(QueryResult {
                columns,
                rows,
                truncated,
                elapsed_ms: started.elapsed().as_millis() as u64,
            })
        })
    }

    /// Run one ad-hoc statement and return at most `max_rows` rows. Reads
    /// are confined to the project folder; writes to files are refused by
    /// the same sandbox.
    pub fn run_sql(
        &self,
        sql: &str,
        max_rows: usize,
        on_start: impl FnOnce(CancelHandle),
    ) -> Result<QueryResult> {
        let sql = sql.trim().trim_end_matches(';');
        check_statement(sql)?;
        let max_rows = max_rows.clamp(1, self.limits.max_rows);
        let started = Instant::now();
        self.guarded(on_start, |conn| {
            let (rows, columns, truncated) = collect_rows(conn, sql, &[], max_rows)?;
            Ok(QueryResult {
                columns,
                rows,
                truncated,
                elapsed_ms: started.elapsed().as_millis() as u64,
            })
        })
    }
}

/// Statements the SQL editor may run. The sandbox already confines file
/// access to the project, but within it DuckDB could still *write*
/// (`COPY … TO`), attach databases, or change settings; the editor is for
/// reading data, so only these leading keywords are accepted. Everything
/// else (DDL/DML included) only touches the in-memory session database.
const ALLOWED_STATEMENTS: &[&str] = &[
    "SELECT",
    "WITH",
    "FROM",
    "VALUES",
    "TABLE",
    "DESCRIBE",
    "SUMMARIZE",
    "SHOW",
    "EXPLAIN",
    "PIVOT",
    "UNPIVOT",
    "CREATE",
    "DROP",
    "INSERT",
    "UPDATE",
    "DELETE",
    "ALTER",
];

/// Strip leading whitespace and comments, then check the first keyword.
fn check_statement(sql: &str) -> Result<()> {
    let mut rest = sql;
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix("--") {
            rest = after.split_once('\n').map_or("", |(_, r)| r);
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after.split_once("*/").map_or("", |(_, r)| r);
        } else {
            break;
        }
    }
    let keyword: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    if keyword.is_empty() {
        return Err(DataError::Invalid("the query is empty".into()));
    }
    let analyze = keyword == "EXPLAIN"
        && rest[7..]
            .trim_start()
            .to_ascii_uppercase()
            .starts_with("ANALYZE");
    if !ALLOWED_STATEMENTS.contains(&keyword.as_str()) || analyze {
        return Err(DataError::Invalid(format!(
            "{keyword} statements aren't allowed in the SQL editor; it reads data, it doesn't write files or change settings"
        )));
    }
    // duckdb-rs prepares a multi-statement string by running all but the
    // last, so `SELECT 1; COPY …` would slip past the keyword check above.
    if count_statements(sql) > 1 {
        return Err(DataError::Invalid("run one statement at a time".into()));
    }
    Ok(())
}

/// Count `;`-separated statements, ignoring semicolons inside quotes,
/// dollar-quoted strings and comments.
fn count_statements(sql: &str) -> usize {
    let b = sql.as_bytes();
    let (mut i, mut count, mut has_content) = (0, 0, false);
    while i < b.len() {
        match b[i] {
            q @ (b'\'' | b'"') => {
                has_content = true;
                i += 1;
                while i < b.len() {
                    if b[i] == q {
                        // A doubled quote is an escaped quote.
                        if b.get(i + 1) == Some(&q) {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            b'$' => {
                // $tag$ … $tag$
                let start = i;
                let mut j = i + 1;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                if j < b.len() && b[j] == b'$' {
                    let tag = &sql[start..=j];
                    has_content = true;
                    i = sql[j + 1..]
                        .find(tag)
                        .map_or(b.len(), |k| j + 1 + k + tag.len());
                } else {
                    i += 1;
                }
            }
            b';' => {
                if has_content {
                    count += 1;
                }
                has_content = false;
                i += 1;
            }
            c => {
                if !c.is_ascii_whitespace() {
                    has_content = true;
                }
                i += 1;
            }
        }
    }
    count + usize::from(has_content)
}

fn describe_columns(conn: &Connection, src: &str) -> Result<Vec<Column>> {
    let mut stmt = conn.prepare(&format!("DESCRIBE SELECT * FROM {src}"))?;
    let columns = stmt
        .query_map([], |r| {
            Ok(Column {
                name: r.get(0)?,
                data_type: r.get(1)?,
            })
        })?
        .collect::<duckdb::Result<Vec<_>>>()?;
    Ok(columns)
}

type Rows = Vec<Vec<serde_json::Value>>;

/// Execute and read at most `max` rows, reporting whether more existed.
fn collect_rows(
    conn: &Connection,
    sql: &str,
    params: &[String],
    max: usize,
) -> Result<(Rows, Vec<Column>, bool)> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let stmt = rows.as_ref().expect("rows from a statement");
    let count = stmt.column_count();
    let columns: Vec<Column> = (0..count)
        .map(|i| Column {
            name: stmt
                .column_name(i)
                .map(|s| s.to_string())
                .unwrap_or_default(),
            data_type: sql_type_name(&format!("{:?}", stmt.column_type(i))),
        })
        .collect();
    let mut out = Vec::new();
    let mut truncated = false;
    while let Some(row) = rows.next()? {
        if out.len() == max {
            truncated = true;
            break;
        }
        let mut values = Vec::with_capacity(count);
        for i in 0..count {
            values.push(values::to_json(row.get::<_, duckdb::types::Value>(i)?));
        }
        out.push(values);
    }
    Ok((out, columns, truncated))
}

/// Result columns arrive with Arrow types; show the SQL names people know.
fn sql_type_name(arrow: &str) -> String {
    let head = arrow.split(['(', ' ']).next().unwrap_or(arrow);
    let name = match head {
        "Boolean" => "BOOLEAN",
        "Int8" => "TINYINT",
        "Int16" => "SMALLINT",
        "Int32" => "INTEGER",
        "Int64" => "BIGINT",
        "UInt8" => "UTINYINT",
        "UInt16" => "USMALLINT",
        "UInt32" => "UINTEGER",
        "UInt64" => "UBIGINT",
        "Float16" | "Float32" => "FLOAT",
        "Float64" => "DOUBLE",
        "Utf8" | "LargeUtf8" | "Utf8View" => "VARCHAR",
        "Binary" | "LargeBinary" | "BinaryView" | "FixedSizeBinary" => "BLOB",
        "Date32" | "Date64" => "DATE",
        "Time32" | "Time64" => "TIME",
        "Timestamp" => "TIMESTAMP",
        "Interval" | "Duration" => "INTERVAL",
        "Decimal128" | "Decimal256" => "DECIMAL",
        "List" | "LargeList" | "FixedSizeList" => "LIST",
        "Struct" => "STRUCT",
        "Map" => "MAP",
        "Dictionary" => "ENUM",
        "Null" => "NULL",
        other => return other.to_uppercase(),
    };
    name.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Contains,
    StartsWith,
    IsNull,
    NotNull,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub column: String,
    pub op: FilterOp,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sort {
    pub column: String,
    #[serde(default)]
    pub desc: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRequest {
    pub offset: u64,
    pub limit: usize,
    #[serde(default)]
    pub sort: Vec<Sort>,
    #[serde(default)]
    pub filters: Vec<Filter>,
    /// Also count the rows matching the filters.
    #[serde(default)]
    pub with_total: bool,
}

/// `WHERE …` with `?` placeholders for every user-supplied value.
fn filters_sql(filters: &[Filter], known: &dyn Fn(&str) -> bool) -> Result<(String, Vec<String>)> {
    let mut clauses = Vec::new();
    let mut params = Vec::new();
    for f in filters {
        if !known(&f.column) {
            return Err(DataError::Invalid(format!("no column named {}", f.column)));
        }
        let col = ident(&f.column);
        let text = format!("lower(CAST({col} AS VARCHAR))");
        let mut needs_value = |sql: String| -> Result<String> {
            let v = f
                .value
                .clone()
                .ok_or_else(|| DataError::Invalid(format!("{:?} needs a value", f.op)))?;
            params.push(v);
            Ok(sql)
        };
        let clause = match f.op {
            FilterOp::IsNull => format!("{col} IS NULL"),
            FilterOp::NotNull => format!("{col} IS NOT NULL"),
            FilterOp::Eq => needs_value(format!("CAST({col} AS VARCHAR) = ?"))?,
            FilterOp::Ne => needs_value(format!("CAST({col} AS VARCHAR) IS DISTINCT FROM ?"))?,
            FilterOp::Lt => needs_value(format!("{col} < ?"))?,
            FilterOp::Le => needs_value(format!("{col} <= ?"))?,
            FilterOp::Gt => needs_value(format!("{col} > ?"))?,
            FilterOp::Ge => needs_value(format!("{col} >= ?"))?,
            FilterOp::Contains => needs_value(format!("contains({text}, lower(?))"))?,
            FilterOp::StartsWith => needs_value(format!("starts_with({text}, lower(?))"))?,
        };
        clauses.push(clause);
    }
    let sql = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    Ok((sql, params))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetInfo {
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
    pub columns: Vec<Column>,
    pub row_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub offset: u64,
    pub columns: Vec<Column>,
    pub rows: Rows,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<Column>,
    pub rows: Rows,
    /// More rows existed than were returned.
    pub truncated: bool,
    pub elapsed_ms: u64,
}

#[cfg(test)]
mod tests;
