//! The local runtime against real child processes.
//!
//! The child is this test binary, re-run with a filter that selects only
//! `fixture` and `EPOCH_RUNTIME_FIXTURE` naming what it should do, so the
//! tests need no Python or other tool on the machine.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use epoch_runtime::{
    Control, JobEvent, JobId, JobKind, JobNotice, JobSpec, JobStatus, LaunchConsent, LocalRuntime,
    ProtocolEvent, Runtime, RuntimeOptions, StopMode,
};

const MODE: &str = "EPOCH_RUNTIME_FIXTURE";
const HEARTBEAT: &str = "EPOCH_RUNTIME_HEARTBEAT";
const LONG: Duration = Duration::from_secs(20);

fn out(line: &str) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{line}");
    let _ = stdout.flush();
}

/// The child's side. Does nothing in a normal test run.
#[test]
fn fixture() {
    let Ok(mode) = std::env::var(MODE) else {
        return;
    };
    // libtest has printed "test fixture ... " without a newline; end it so
    // the first protocol line stands alone.
    out("");
    let started = r#"{"ev":"run.started","run":"r1","totalEpochs":2,"device":"cpu","protocol":1}"#;
    match mode.as_str() {
        // A short protocol run plus plain output on both streams.
        "train" => {
            out(started);
            out("plain stdout line");
            eprintln!("a warning on stderr");
            out(r#"{"ev":"epoch","run":"r1","epoch":1,"of":2}"#);
            out(r#"{"ev":"metric","run":"r1","epoch":1,"step":1,"name":"loss","value":0.5}"#);
            out(r#"{"ev":"metric","run":"r1","epoch":2,"step":2,"name":"loss","value":0.25}"#);
            out(r#"{"ev":"run.finished","run":"r1","status":"completed"}"#);
            std::process::exit(3);
        }
        // Waits for control messages like an engine does.
        "obedient" => {
            out(started);
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                let line = line.unwrap_or_default();
                if line.contains("\"pause\"") {
                    out(r#"{"ev":"log","run":"r1","level":"info","msg":"paused"}"#);
                } else if line.contains("\"resume\"") {
                    out(r#"{"ev":"log","run":"r1","level":"info","msg":"resumed"}"#);
                } else if line.contains("\"cancel\"") {
                    out(r#"{"ev":"run.finished","run":"r1","status":"cancelled"}"#);
                    std::process::exit(0);
                }
            }
            std::process::exit(9);
        }
        // Ignores cancel, and starts a grandchild that writes a heartbeat
        // file, to prove the kill reaches the whole tree.
        "stubborn" => {
            let exe = std::env::current_exe().unwrap();
            let mut grandchild = std::process::Command::new(exe);
            grandchild
                .args(["fixture", "--exact", "--nocapture", "--test-threads=1"])
                .env(MODE, "heartbeat")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            let _child = grandchild.spawn().unwrap();
            out(started);
            std::thread::sleep(Duration::from_secs(60));
            std::process::exit(0);
        }
        "heartbeat" => {
            let file = PathBuf::from(std::env::var(HEARTBEAT).unwrap());
            for i in 0..600u32 {
                let _ = std::fs::write(&file, i.to_string());
                std::thread::sleep(Duration::from_millis(50));
            }
            std::process::exit(0);
        }
        other => panic!("unknown fixture mode {other}"),
    }
}

fn spec(mode: &str, cwd: &Path, extra_env: Vec<(String, String)>) -> JobSpec {
    let mut env = vec![(MODE.to_string(), mode.to_string())];
    env.extend(extra_env);
    JobSpec {
        title: mode.into(),
        scope: "tests".into(),
        kind: JobKind::Process {
            program: std::env::current_exe().unwrap(),
            args: ["fixture", "--exact", "--nocapture", "--test-threads=1"]
                .map(String::from)
                .to_vec(),
            speaks_protocol: true,
        },
        cwd: cwd.to_path_buf(),
        env,
        consent: LaunchConsent::from_user_action("test", 0),
    }
}

fn all_events(rt: &LocalRuntime, id: JobId) -> Vec<JobEvent> {
    let mut after = 0;
    let mut events = Vec::new();
    loop {
        let page = rt.events(id, after, 3).unwrap();
        if page.records.is_empty() {
            break;
        }
        after = page.next;
        events.extend(page.records.into_iter().map(|r| r.event));
    }
    events
}

/// Poll until `check` passes, or fail after `LONG`.
fn eventually(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + LONG;
    while !check() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn saw_log(rt: &LocalRuntime, id: JobId, msg: &str) -> bool {
    all_events(rt, id).iter().any(|e| {
        matches!(e, JobEvent::Protocol { event: ProtocolEvent::Log { msg: m, .. } } if m == msg)
    })
}

#[test]
fn streams_output_and_protocol_events_then_reports_the_exit_code() {
    let tmp = tempfile::tempdir().unwrap();
    let rt = LocalRuntime::default();
    let notices = Arc::new(Mutex::new(Vec::new()));
    let sink = notices.clone();
    rt.set_observer(Arc::new(move |_, notice| {
        sink.lock().unwrap().push(notice.clone())
    }));

    let id = rt.start(spec("train", tmp.path(), vec![]));
    let status = rt.wait(id, LONG).unwrap();
    assert_eq!(status, JobStatus::Exited { code: Some(3) });

    let events = all_events(&rt, id);
    let stdout: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            JobEvent::Stdout { line } => Some(line.as_str()),
            _ => None,
        })
        .collect();
    assert!(stdout.contains(&"plain stdout line"), "{stdout:?}");
    assert!(events
        .iter()
        .any(|e| matches!(e, JobEvent::Stderr { line } if line == "a warning on stderr")));
    let metrics: Vec<f64> = events
        .iter()
        .filter_map(|e| match e {
            JobEvent::Protocol {
                event: ProtocolEvent::Metric { value, .. },
            } => Some(*value),
            _ => None,
        })
        .collect();
    assert_eq!(metrics, [0.5, 0.25]);
    // The final status is the last event, after all output.
    assert_eq!(events.last(), Some(&JobEvent::Status(status.clone())));

    let info = rt.info(id).unwrap();
    assert_eq!(info.progress.run_id.as_deref(), Some("r1"));
    assert_eq!(
        info.progress.run_dir,
        Some(tmp.path().join(".nexis-ml").join("runs").join("r1"))
    );
    assert_eq!(info.progress.epoch, Some(2));
    assert_eq!(info.progress.total_epochs, Some(2));
    assert_eq!(info.progress.outcome.as_deref(), Some("completed"));
    assert_eq!(info.progress.metric_events, 2);

    let notices = notices.lock().unwrap().clone();
    assert!(matches!(&notices[0], JobNotice::RunStarted { run_id, .. } if run_id == "r1"));
    assert_eq!(notices.last(), Some(&JobNotice::Finished(status)));
}

#[test]
fn the_cursor_resumes_and_reports_dropped_events() {
    let tmp = tempfile::tempdir().unwrap();
    let rt = LocalRuntime::new(RuntimeOptions {
        log_capacity: 4,
        ..Default::default()
    });
    let id = rt.start(spec("train", tmp.path(), vec![]));
    rt.wait(id, LONG).unwrap();
    let page = rt.events(id, 0, 100).unwrap();
    assert_eq!(page.records.len(), 4);
    assert!(page.dropped > 0);
    assert!(page.done);
    let again = rt.events(id, page.next, 100).unwrap();
    assert!(again.records.is_empty() && again.done && again.dropped == 0);
}

#[test]
fn pause_resume_and_graceful_cancel_go_through_stdin() {
    let tmp = tempfile::tempdir().unwrap();
    let rt = LocalRuntime::default();
    let id = rt.start(spec("obedient", tmp.path(), vec![]));
    eventually("the run to start", || {
        rt.info(id).unwrap().progress.run_id.is_some()
    });

    rt.control(id, Control::Pause).unwrap();
    assert!(matches!(rt.status(id).unwrap(), JobStatus::Paused { .. }));
    eventually("the pause to arrive", || saw_log(&rt, id, "paused"));
    rt.control(id, Control::Resume).unwrap();
    assert!(matches!(rt.status(id).unwrap(), JobStatus::Running { .. }));
    eventually("the resume to arrive", || saw_log(&rt, id, "resumed"));

    rt.stop(
        id,
        StopMode::Graceful {
            timeout: Duration::from_secs(15),
        },
    )
    .unwrap();
    assert_eq!(
        rt.wait(id, LONG).unwrap(),
        JobStatus::Cancelled { forced: false }
    );
    assert_eq!(
        rt.info(id).unwrap().progress.outcome.as_deref(),
        Some("cancelled")
    );
    assert!(rt.control(id, Control::Pause).is_err());
}

#[test]
fn a_process_that_ignores_cancel_is_killed_with_its_children_after_the_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    let heartbeat = tmp.path().join("heartbeat.txt");
    let rt = LocalRuntime::default();
    let env = vec![(HEARTBEAT.to_string(), heartbeat.display().to_string())];
    let id = rt.start(spec("stubborn", tmp.path(), env));
    eventually("the grandchild's heartbeat", || heartbeat.is_file());

    let asked = Instant::now();
    rt.stop(
        id,
        StopMode::Graceful {
            timeout: Duration::from_millis(400),
        },
    )
    .unwrap();
    assert!(matches!(rt.status(id).unwrap(), JobStatus::Stopping { .. }));
    assert_eq!(
        rt.wait(id, LONG).unwrap(),
        JobStatus::Cancelled { forced: true }
    );
    let took = asked.elapsed();
    assert!(took >= Duration::from_millis(400), "killed early: {took:?}");
    assert!(took < Duration::from_secs(10), "killed late: {took:?}");

    // The grandchild died with the job: its heartbeat stops.
    std::thread::sleep(Duration::from_millis(300));
    let before = std::fs::read_to_string(&heartbeat).unwrap_or_default();
    std::thread::sleep(Duration::from_millis(500));
    let after = std::fs::read_to_string(&heartbeat).unwrap_or_default();
    assert_eq!(before, after, "the grandchild is still running");
}

#[test]
fn kill_is_immediate() {
    let tmp = tempfile::tempdir().unwrap();
    let rt = LocalRuntime::default();
    let id = rt.start(spec("obedient", tmp.path(), vec![]));
    eventually("the process", || rt.status(id).unwrap().pid().is_some());
    rt.stop(id, StopMode::Kill).unwrap();
    assert_eq!(
        rt.wait(id, LONG).unwrap(),
        JobStatus::Cancelled { forced: true }
    );
}

#[test]
fn jobs_beyond_the_limit_wait_their_turn() {
    let tmp = tempfile::tempdir().unwrap();
    let rt = LocalRuntime::new(RuntimeOptions {
        max_concurrent: 1,
        ..Default::default()
    });
    let first = rt.start(spec("obedient", tmp.path(), vec![]));
    let second = rt.start(spec("obedient", tmp.path(), vec![]));
    let third = rt.start(spec("obedient", tmp.path(), vec![]));
    assert_eq!(rt.status(second).unwrap(), JobStatus::Queued);

    // A queued job can be cancelled without ever starting.
    rt.stop(third, StopMode::Kill).unwrap();
    assert_eq!(
        rt.status(third).unwrap(),
        JobStatus::Cancelled { forced: false }
    );
    assert!(rt.info(third).unwrap().started_at_ms.is_none());

    eventually("the first job", || {
        rt.status(first).unwrap().pid().is_some()
    });
    assert_eq!(rt.status(second).unwrap(), JobStatus::Queued);
    rt.control(first, Control::Cancel).unwrap();
    rt.wait(first, LONG).unwrap();
    eventually("the second job to start", || {
        rt.status(second).unwrap().pid().is_some()
    });
    rt.stop(second, StopMode::Kill).unwrap();
    rt.wait(second, LONG).unwrap();
    // Newest first.
    let ids: Vec<_> = rt.jobs().iter().map(|j| j.id).collect();
    assert_eq!(ids, [third, second, first]);
}

#[test]
fn a_missing_program_fails_without_taking_a_slot() {
    let tmp = tempfile::tempdir().unwrap();
    let rt = LocalRuntime::new(RuntimeOptions {
        max_concurrent: 1,
        ..Default::default()
    });
    let mut bad = spec("train", tmp.path(), vec![]);
    bad.kind = JobKind::Process {
        program: tmp.path().join("no-such-program"),
        args: vec![],
        speaks_protocol: false,
    };
    let bad = rt.start(bad);
    let good = rt.start(spec("train", tmp.path(), vec![]));
    assert!(matches!(rt.status(bad).unwrap(), JobStatus::Failed { .. }));
    assert_eq!(
        rt.wait(good, LONG).unwrap(),
        JobStatus::Exited { code: Some(3) }
    );
}

#[test]
fn dropping_the_runtime_kills_its_jobs() {
    let tmp = tempfile::tempdir().unwrap();
    let heartbeat = tmp.path().join("heartbeat.txt");
    let rt = LocalRuntime::default();
    let env = vec![(HEARTBEAT.to_string(), heartbeat.display().to_string())];
    rt.start(spec("stubborn", tmp.path(), env));
    eventually("the grandchild's heartbeat", || heartbeat.is_file());
    drop(rt);
    std::thread::sleep(Duration::from_millis(500));
    let before = std::fs::read_to_string(&heartbeat).unwrap_or_default();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        before,
        std::fs::read_to_string(&heartbeat).unwrap_or_default()
    );
}
