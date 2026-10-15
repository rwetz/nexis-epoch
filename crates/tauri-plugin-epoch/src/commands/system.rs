//! CPU, memory and (NVIDIA) GPU facts for the status bar.
//!
//! GPU figures come from `nvidia-smi`, a system tool, when it is on PATH.
//! That is the only process Epoch starts on its own, and it never starts
//! anything from a project.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::State;

use crate::state::AppState;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuStatus {
    pub name: String,
    pub utilization_percent: Option<f32>,
    pub memory_used_mb: Option<u64>,
    pub memory_total_mb: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub cpu_percent: f32,
    pub cpu_count: usize,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub gpus: Vec<GpuStatus>,
    /// `false` when no supported GPU tool was found.
    pub gpu_available: bool,
    pub running_queries: usize,
}

pub struct SystemProbe {
    sys: sysinfo::System,
    gpus: Vec<GpuStatus>,
    gpu_checked: Option<Instant>,
    gpu_missing: bool,
}

const GPU_REFRESH: Duration = Duration::from_secs(5);

impl SystemProbe {
    pub fn new() -> Self {
        Self {
            sys: sysinfo::System::new(),
            gpus: Vec::new(),
            gpu_checked: None,
            gpu_missing: false,
        }
    }

    fn refresh_gpus(&mut self) {
        if self.gpu_missing || self.gpu_checked.is_some_and(|t| t.elapsed() < GPU_REFRESH) {
            return;
        }
        self.gpu_checked = Some(Instant::now());
        let mut cmd = std::process::Command::new("nvidia-smi");
        cmd.args([
            "--query-gpu=name,utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        match cmd.output() {
            Ok(out) if out.status.success() => {
                self.gpus = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter_map(|line| {
                        let f: Vec<&str> = line.split(',').map(str::trim).collect();
                        (f.len() == 4).then(|| GpuStatus {
                            name: f[0].to_string(),
                            utilization_percent: f[1].parse().ok(),
                            memory_used_mb: f[2].parse().ok(),
                            memory_total_mb: f[3].parse().ok(),
                        })
                    })
                    .collect();
            }
            Ok(_) => self.gpus.clear(),
            Err(_) => {
                self.gpu_missing = true;
                self.gpus.clear();
            }
        }
    }
}

#[tauri::command]
pub async fn system_status(
    state: State<'_, Arc<AppState>>,
) -> Result<SystemStatus, crate::error::CommandError> {
    let state = state.inner().clone();
    crate::error::blocking(move || {
        let running_queries = state
            .queries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len();
        let mut probe = state.system.lock().unwrap_or_else(|e| e.into_inner());
        probe.sys.refresh_cpu_usage();
        probe.sys.refresh_memory();
        probe.refresh_gpus();
        Ok(SystemStatus {
            cpu_percent: probe.sys.global_cpu_usage(),
            cpu_count: probe.sys.cpus().len(),
            memory_used_mb: probe.sys.used_memory() / 1_048_576,
            memory_total_mb: probe.sys.total_memory() / 1_048_576,
            gpu_available: !probe.gpu_missing,
            gpus: probe.gpus.clone(),
            running_queries,
        })
    })
    .await
}
