//! Reads a real Nexis ML project when one is named, to catch drift between
//! the fixtures in `run_store.rs` and what the engines actually write.
//!
//!     EPOCH_NEXIS_ML_PROJECT=/path/to/project cargo test -p epoch-core --test real_run_store
//!
//! Without the variable the test passes without doing anything.

use std::path::PathBuf;

use epoch_core::nexis_ml::run_store::{list_runs, read_artifact_json, read_run_detail};
use epoch_core::project::detect;
use epoch_core::ProjectRoot;

#[test]
fn reads_a_real_project() {
    let Some(dir) = std::env::var_os("EPOCH_NEXIS_ML_PROJECT").map(PathBuf::from) else {
        eprintln!("EPOCH_NEXIS_ML_PROJECT not set; skipping");
        return;
    };
    let root = ProjectRoot::open(&dir).expect("project folder");
    let summary = detect(&root);
    assert!(
        !summary.experiment_stores.is_empty(),
        "no .nexis-ml/runs found"
    );

    let runs = list_runs(root.path()).expect("runs");
    assert!(!runs.is_empty());
    let mut checked = 0;
    for run in &runs {
        let detail = read_run_detail(root.path(), std::path::Path::new(&run.dir)).expect("detail");
        if run.status == "ok" {
            assert!(
                !detail.series.is_empty(),
                "{}: finished run without metrics",
                run.id
            );
        }
        for artifact in detail
            .artifacts
            .iter()
            .filter(|a| a.kind == "confusion-matrix")
        {
            let rel = artifact
                .path
                .as_deref()
                .expect("artifact rebased into the run");
            assert!(artifact.exists, "{}: {rel} missing", run.id);
            let cm = read_artifact_json(std::path::Path::new(&run.dir), rel).expect("readable");
            assert!(
                cm["matrix"].is_array() && cm["labels"].is_array(),
                "{rel}: {cm}"
            );
        }
        checked += 1;
    }
    eprintln!(
        "{}: {} runs, {} stores, newest {} ({} metrics)",
        summary.name,
        checked,
        summary.experiment_stores.len(),
        runs[0].id,
        runs[0].metrics.len()
    );
}
