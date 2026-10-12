use super::*;

fn no_cancel(_: CancelHandle) {}

/// A project with a CSV and a Parquet file of the same rows, plus a file
/// outside the project that queries must not be able to read.
fn fixture() -> (tempfile::TempDir, tempfile::TempDir, DataEngine) {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("data")).unwrap();
    let mut csv = String::from("id,name,score,joined\n");
    for i in 0..250 {
        csv.push_str(&format!(
            "{i},player {i},{},2024-01-{:02}\n",
            (i * 7) % 100,
            i % 28 + 1
        ));
    }
    csv.push_str("250,,,2024-02-01\n");
    std::fs::write(dir.path().join("data/players.csv"), &csv).unwrap();
    std::fs::write(outside.path().join("secret.csv"), "token\nhunter2\n").unwrap();

    // Write the Parquet copy with an unrestricted connection, as a user's
    // own pipeline would have.
    let writer = Connection::open_in_memory().unwrap();
    let csv_path = dir.path().join("data/players.csv").display().to_string();
    let pq_path = dir
        .path()
        .join("data/players.parquet")
        .display()
        .to_string();
    writer
        .execute_batch(&format!(
            "COPY (SELECT * FROM read_csv({})) TO {} (FORMAT parquet)",
            literal(&csv_path),
            literal(&pq_path)
        ))
        .unwrap();

    let root = ProjectRoot::open(dir.path()).unwrap();
    let engine = DataEngine::open(
        &root,
        Limits {
            timeout: Duration::from_secs(20),
            ..Limits::default()
        },
    )
    .unwrap();
    (dir, outside, engine)
}

#[test]
fn describes_csv_and_parquet() {
    let (_d, _o, engine) = fixture();
    for file in ["data/players.csv", "data/players.parquet"] {
        let info = engine.describe(file, no_cancel).unwrap();
        assert_eq!(info.row_count, 251, "{file}");
        let names: Vec<_> = info.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["id", "name", "score", "joined"], "{file}");
        assert_eq!(info.columns[3].data_type, "DATE", "{file}");
    }
}

#[test]
fn pages_sort_and_filter_with_bound_values() {
    let (_d, _o, engine) = fixture();
    let req = PageRequest {
        offset: 0,
        limit: 5,
        sort: vec![Sort {
            column: "score".into(),
            desc: true,
        }],
        filters: vec![Filter {
            column: "score".into(),
            op: FilterOp::Ge,
            value: Some("90".into()),
        }],
        with_total: true,
    };
    let page = engine.page("data/players.csv", &req, no_cancel).unwrap();
    assert_eq!(page.rows.len(), 5);
    assert_eq!(page.rows[0][2], serde_json::json!(99));
    let joined = page.rows[0][3].as_str().unwrap();
    assert!(
        joined.starts_with("2024-01-") && joined.len() == 10,
        "{joined}"
    );
    assert!(page.total.unwrap() > 5);

    let contains = PageRequest {
        offset: 0,
        limit: 50,
        sort: vec![],
        // A quote in the value must be data, not SQL.
        filters: vec![Filter {
            column: "name".into(),
            op: FilterOp::Contains,
            value: Some("ER 24'".into()),
        }],
        with_total: true,
    };
    assert_eq!(
        engine
            .page("data/players.csv", &contains, no_cancel)
            .unwrap()
            .total,
        Some(0)
    );
    let nulls = PageRequest {
        offset: 0,
        limit: 50,
        sort: vec![],
        filters: vec![Filter {
            column: "score".into(),
            op: FilterOp::IsNull,
            value: None,
        }],
        with_total: true,
    };
    assert_eq!(
        engine
            .page("data/players.csv", &nulls, no_cancel)
            .unwrap()
            .total,
        Some(1)
    );
}

#[test]
fn rejects_unknown_columns() {
    let (_d, _o, engine) = fixture();
    let req = PageRequest {
        offset: 0,
        limit: 5,
        sort: vec![Sort {
            column: "score\"; DROP TABLE x; --".into(),
            desc: false,
        }],
        filters: vec![],
        with_total: false,
    };
    assert!(matches!(
        engine.page("data/players.csv", &req, no_cancel),
        Err(DataError::Invalid(_))
    ));
}

#[test]
fn summarizes_columns() {
    let (_d, _o, engine) = fixture();
    let stats = engine.stats("data/players.parquet", no_cancel).unwrap();
    assert_eq!(stats.rows.len(), 4);
    assert_eq!(stats.columns[0].name, "column_name");
}

#[test]
fn sql_reads_project_datasets_through_views() {
    let (dir, _o, engine) = fixture();
    let name = engine
        .register_view("data/players.parquet", "players")
        .unwrap();
    assert_eq!(name, "players");
    let result = engine
        .run_sql("SELECT count(*) AS n FROM players", 100, no_cancel)
        .unwrap();
    assert_eq!(result.rows[0][0], serde_json::json!(251));
    assert_eq!(result.columns[0].data_type, "BIGINT");
    // Absolute paths inside the project work too.
    let abs = dir.path().join("data/players.csv").display().to_string();
    let result = engine
        .run_sql(
            &format!("SELECT count(*) FROM {}", literal(&abs)),
            10,
            no_cancel,
        )
        .unwrap();
    assert_eq!(result.rows[0][0], serde_json::json!(251));
    assert_eq!(
        engine
            .register_view("data/players.csv", "2024 players!")
            .unwrap(),
        "t_2024_players_"
    );
}

#[test]
fn sql_caps_rows() {
    let (_d, _o, engine) = fixture();
    let result = engine
        .run_sql("SELECT * FROM range(1000)", 10, no_cancel)
        .unwrap();
    assert_eq!(result.rows.len(), 10);
    assert!(result.truncated);
}

#[test]
fn sandbox_blocks_files_outside_the_project() {
    let (_d, outside, engine) = fixture();
    let secret = outside.path().join("secret.csv").display().to_string();
    let err = engine.run_sql(
        &format!("SELECT * FROM read_csv({})", literal(&secret)),
        10,
        no_cancel,
    );
    assert!(err.is_err(), "read outside the project: {err:?}");
    let err = engine.run_sql(
        &format!("SELECT * FROM {}", literal(&secret)),
        10,
        no_cancel,
    );
    assert!(err.is_err());
}

#[test]
fn sandbox_cannot_be_switched_off() {
    let (_d, _o, engine) = fixture();
    // Refused by the editor's statement check…
    assert!(engine
        .run_sql("SET enable_external_access = true", 10, no_cancel)
        .is_err());
    // …and by DuckDB itself, which has the configuration locked.
    let conn = engine.connection().unwrap();
    assert!(conn
        .execute_batch("SET enable_external_access = true")
        .is_err());
    assert!(conn
        .execute_batch("SET lock_configuration = false")
        .is_err());
}

#[test]
fn editor_refuses_writes_and_multiple_statements() {
    let (dir, _o, engine) = fixture();
    let target = dir.path().join("data/out.csv").display().to_string();
    for sql in [
        format!("COPY (SELECT 1) TO {}", literal(&target)),
        "/* hi */ -- there\n  copy (select 1) to 'x.csv'".to_string(),
        "ATTACH 'other.db'".to_string(),
        "INSTALL httpfs".to_string(),
        "EXPLAIN ANALYZE SELECT 1".to_string(),
        "SELECT 1; SELECT 2".to_string(),
        format!("SELECT 1; COPY (SELECT 1) TO {}", literal(&target)),
    ] {
        assert!(engine.run_sql(&sql, 10, no_cancel).is_err(), "{sql}");
    }
    assert!(!dir.path().join("data/out.csv").exists());
    // Semicolons inside strings, identifiers and comments are fine, as is
    // a trailing one.
    for sql in [
        "SELECT 'a;b' AS s",
        "SELECT 1 AS \"x;y\" -- ;
",
        "SELECT 1; /* ; */",
        "SELECT 'it''s;' AS s;",
    ] {
        assert!(engine.run_sql(sql, 10, no_cancel).is_ok(), "{sql}");
    }
    // Session tables live in memory and persist across queries.
    engine
        .run_sql("CREATE TABLE t AS SELECT 42 AS x", 10, no_cancel)
        .unwrap();
    let r = engine.run_sql("SELECT x FROM t", 10, no_cancel).unwrap();
    assert_eq!(r.rows[0][0], serde_json::json!(42));
}

#[test]
fn queries_time_out() {
    let (_d, _o, mut engine) = fixture();
    engine.limits.timeout = Duration::from_millis(200);
    let slow = "SELECT count(*) FROM range(10000000000) a";
    let started = Instant::now();
    let err = engine.run_sql(slow, 10, no_cancel);
    assert!(matches!(err, Err(DataError::Timeout(_))), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn queries_can_be_cancelled_from_another_thread() {
    let (_d, _o, engine) = fixture();
    let (tx, rx) = mpsc::channel();
    let canceller = std::thread::spawn(move || {
        let handle: CancelHandle = rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(150));
        handle.cancel();
    });
    let err = engine.run_sql("SELECT count(*) FROM range(10000000000) a", 10, |h| {
        tx.send(h).unwrap()
    });
    canceller.join().unwrap();
    assert!(matches!(err, Err(DataError::Cancelled)), "{err:?}");
}

#[test]
fn counts_statements_like_a_tokenizer() {
    assert_eq!(count_statements("SELECT 1"), 1);
    assert_eq!(count_statements("SELECT 1;"), 1);
    assert_eq!(count_statements("SELECT 1; SELECT 2"), 2);
    assert_eq!(count_statements("SELECT ';' ; "), 1);
    assert_eq!(count_statements("SELECT $$a;b$$, $q$;$q$"), 1);
    assert_eq!(
        count_statements(
            "-- c;
SELECT 1 /* ; */"
        ),
        1
    );
    assert_eq!(count_statements(";;"), 0);
}

#[test]
fn engine_can_be_shared_across_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DataEngine>();
}

#[test]
fn reads_json_lines() {
    let (dir, _o, engine) = fixture();
    std::fs::write(
        dir.path().join("data/events.jsonl"),
        "{\"step\":1,\"loss\":0.9}\n{\"step\":2,\"loss\":0.5}\n",
    )
    .unwrap();
    let info = engine.describe("data/events.jsonl", no_cancel).unwrap();
    assert_eq!(info.row_count, 2);
    assert_eq!(info.columns.len(), 2);
}
