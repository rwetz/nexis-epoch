//! [`LocalRuntime`]: jobs as child processes of this one.
//!
//! Threads, not async: each running job has a stdout reader, a stderr
//! reader and a waiter, and a stop with a timeout adds a short-lived timer.
//! Callers (the Tauri plugin) poll [`LocalRuntime::events`] with a cursor,
//! so a reloaded webview picks up where it left off, and an optional
//! [`Observer`] hears about runs starting and jobs finishing.

use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use epoch_core::nexis_ml;

use crate::lines::read_lines;
use crate::sys::{self, ProcessGroup};
use crate::{
    classify_stdout, Control, EventPage, JobEvent, JobId, JobInfo, JobKind, JobProgress, JobRecord,
    JobSpec, JobStatus, ProtocolEvent, Runtime, RuntimeError, StopMode, DEFAULT_STOP_TIMEOUT,
};

#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    /// Jobs with a live process at once; more wait as `Queued`.
    pub max_concurrent: usize,
    /// Events kept per job (the oldest are dropped first).
    pub log_capacity: usize,
    /// Finished jobs kept in the list (the oldest are forgotten first).
    pub keep_finished: usize,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            max_concurrent: 2,
            log_capacity: 20_000,
            keep_finished: 20,
        }
    }
}

/// Something worth acting on outside the runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum JobNotice {
    /// A protocol engine announced its run (`run.started`).
    RunStarted {
        run_id: String,
        run_dir: PathBuf,
    },
    Finished(JobStatus),
}

/// Called without any runtime lock held, on a runtime thread, before the
/// change it reports is visible to [`LocalRuntime::events`] and
/// [`LocalRuntime::jobs`] (except for a queued job cancelled before it
/// ran, which is reported just after). Keep it short: output waits for it.
pub type Observer = Arc<dyn Fn(&JobInfo, &JobNotice) + Send + Sync>;

/// How often the waiter checks whether the child has exited.
const EXIT_POLL: Duration = Duration::from_millis(20);
/// How long, after the child exits, to wait for its output to drain.
const DRAIN_WAIT: Duration = Duration::from_secs(2);

pub struct LocalRuntime {
    shared: Arc<Shared>,
}

struct Shared {
    opts: RuntimeOptions,
    next_id: AtomicU64,
    jobs: Mutex<BTreeMap<JobId, Arc<Job>>>,
    /// Serialises starting queued jobs, so two finishing jobs can't both
    /// hand the same free slot out.
    pump: Mutex<()>,
    observer: Mutex<Option<Observer>>,
}

struct Job {
    id: JobId,
    spec: JobSpec,
    plan: crate::CommandPlan,
    queued_at_ms: u64,
    inner: Mutex<JobInner>,
    changed: Condvar,
}

struct JobInner {
    status: JobStatus,
    started_at_ms: Option<u64>,
    finished_at_ms: Option<u64>,
    records: VecDeque<JobRecord>,
    next_seq: u64,
    progress: JobProgress,
    output_lines: u64,
    stdin: Option<ChildStdin>,
    process: Option<Arc<Process>>,
    /// A stop was requested; `true` once it had to be forced.
    stop: Option<bool>,
}

struct Process {
    child: Mutex<Child>,
    group: ProcessGroup,
}

impl Process {
    fn kill(&self) {
        self.group.kill();
        let _ = lock(&self.child).kill();
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Job {
    fn lock(&self) -> MutexGuard<'_, JobInner> {
        lock(&self.inner)
    }

    fn info(&self) -> JobInfo {
        let inner = self.lock();
        self.info_locked(&inner)
    }

    fn info_locked(&self, inner: &JobInner) -> JobInfo {
        JobInfo {
            id: self.id,
            title: self.spec.title.clone(),
            scope: self.spec.scope.clone(),
            kind: match self.spec.kind {
                JobKind::Axon { .. } => "axon",
                JobKind::PythonScript { .. } => "python_script",
                JobKind::Process { .. } => "process",
            },
            plan: self.plan.clone(),
            consent: self.spec.consent.clone(),
            status: inner.status.clone(),
            controllable: self.plan.speaks_protocol,
            queued_at_ms: self.queued_at_ms,
            started_at_ms: inner.started_at_ms,
            finished_at_ms: inner.finished_at_ms,
            progress: inner.progress.clone(),
            output_lines: inner.output_lines,
        }
    }
}

impl JobInner {
    fn push(&mut self, event: JobEvent, capacity: usize) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.records.push_back(JobRecord {
            seq,
            at_ms: now_ms(),
            event,
        });
        while self.records.len() > capacity.max(1) {
            self.records.pop_front();
        }
    }

    fn set_status(&mut self, status: JobStatus, capacity: usize) {
        self.status = status.clone();
        self.push(JobEvent::Status(status), capacity);
    }
}

impl Default for LocalRuntime {
    fn default() -> Self {
        Self::new(RuntimeOptions::default())
    }
}

impl LocalRuntime {
    pub fn new(opts: RuntimeOptions) -> Self {
        Self {
            shared: Arc::new(Shared {
                opts,
                next_id: AtomicU64::new(1),
                jobs: Mutex::new(BTreeMap::new()),
                pump: Mutex::new(()),
                observer: Mutex::new(None),
            }),
        }
    }

    pub fn set_observer(&self, observer: Observer) {
        *lock(&self.shared.observer) = Some(observer);
    }

    pub fn info(&self, id: JobId) -> Result<JobInfo, RuntimeError> {
        Ok(self.shared.job(id)?.info())
    }

    /// Block until the job finishes or `timeout` passes; the status either
    /// way.
    pub fn wait(&self, id: JobId, timeout: Duration) -> Result<JobStatus, RuntimeError> {
        let job = self.shared.job(id)?;
        let deadline = Instant::now() + timeout;
        let mut inner = job.lock();
        while !inner.status.is_finished() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            inner = job
                .changed
                .wait_timeout(inner, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        Ok(inner.status.clone())
    }

    /// Kill every job that still has a process, and drop the queue. Called
    /// on drop; the app also calls it when it exits.
    pub fn shutdown(&self) {
        let jobs: Vec<Arc<Job>> = lock(&self.shared.jobs).values().cloned().collect();
        for job in jobs {
            let process = {
                let mut inner = job.lock();
                if matches!(inner.status, JobStatus::Queued) {
                    inner.set_status(
                        JobStatus::Cancelled { forced: false },
                        self.shared.opts.log_capacity,
                    );
                    inner.finished_at_ms = Some(now_ms());
                    job.changed.notify_all();
                }
                if inner.status.holds_slot() {
                    inner.stop = Some(true);
                }
                inner.process.clone()
            };
            if let Some(p) = process {
                p.kill();
            }
        }
    }
}

impl Drop for LocalRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Shared {
    fn job(&self, id: JobId) -> Result<Arc<Job>, RuntimeError> {
        lock(&self.jobs)
            .get(&id)
            .cloned()
            .ok_or(RuntimeError::UnknownJob(id))
    }

    fn notify(&self, job: &Job, notice: JobNotice) {
        let observer = lock(&self.observer).clone();
        if let Some(observer) = observer {
            observer(&job.info(), &notice);
        }
    }

    /// Start queued jobs while there are free slots.
    fn pump(self: &Arc<Self>) {
        let failed = self.pump_turn();
        for (job, status) in failed {
            self.finish(&job, status);
        }
    }

    /// One pass under the pump lock. Jobs that failed to start are returned
    /// so they are finished after the lock is released (finishing pumps).
    fn pump_turn(self: &Arc<Self>) -> Vec<(Arc<Job>, JobStatus)> {
        let _turn = lock(&self.pump);
        let mut failed = Vec::new();
        let jobs: Vec<Arc<Job>> = lock(&self.jobs).values().cloned().collect();
        let mut busy = jobs.iter().filter(|j| j.lock().status.holds_slot()).count();
        for job in jobs {
            if busy >= self.opts.max_concurrent.max(1) {
                break;
            }
            {
                let mut inner = job.lock();
                if !matches!(inner.status, JobStatus::Queued) {
                    continue;
                }
                inner.set_status(JobStatus::Starting, self.opts.log_capacity);
            }
            busy += 1;
            if let Err(status) = self.launch(&job) {
                busy -= 1;
                failed.push((job, status));
            }
        }
        failed
    }

    /// Forget the oldest finished jobs beyond `keep_finished`.
    fn prune(&self) {
        let mut jobs = lock(&self.jobs);
        let finished: Vec<JobId> = jobs
            .values()
            .filter(|j| j.lock().status.is_finished())
            .map(|j| j.id)
            .collect();
        let excess = finished.len().saturating_sub(self.opts.keep_finished);
        for id in finished.into_iter().take(excess) {
            jobs.remove(&id);
        }
    }

    /// Spawn the job's process and its reader and waiter threads. On
    /// failure, the status to finish the job with.
    fn launch(self: &Arc<Self>, job: &Arc<Job>) -> Result<(), JobStatus> {
        let plan = &job.plan;
        let mut cmd = Command::new(&plan.program);
        cmd.args(&plan.args)
            .current_dir(&plan.cwd)
            .envs(plan.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        sys::configure(&mut cmd);

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                let message = RuntimeError::Spawn {
                    program: plan.program.display().to_string(),
                    message: e.to_string(),
                }
                .to_string();
                return Err(JobStatus::Failed { message });
            }
        };
        let group = match ProcessGroup::attach(&child) {
            Ok(group) => group,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(JobStatus::Failed {
                    message: format!("could not contain the process: {e}"),
                });
            }
        };
        let pid = child.id();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let process = Arc::new(Process {
            child: Mutex::new(child),
            group,
        });

        let stop_early = {
            let mut inner = job.lock();
            inner.started_at_ms = Some(now_ms());
            inner.process = Some(process.clone());
            // Only protocol engines read stdin; everyone else gets EOF.
            inner.stdin = if plan.speaks_protocol { stdin } else { None };
            let status = if inner.stop.is_some() {
                JobStatus::Stopping { pid }
            } else {
                JobStatus::Running { pid }
            };
            inner.set_status(status, self.opts.log_capacity);
            job.changed.notify_all();
            inner.stop.is_some()
        };
        if stop_early {
            // Stopped while starting: there is nothing to save yet.
            job.lock().stop = Some(true);
            process.kill();
        }

        let (done_tx, done_rx) = mpsc::channel::<()>();
        if let Some(out) = stdout {
            let (shared, job, done) = (self.clone(), job.clone(), done_tx.clone());
            spawn_thread(format!("epoch-job-{}-stdout", job.id.0), move || {
                read_lines(out, |line| shared.on_stdout(&job, line));
                let _ = done.send(());
            });
        }
        if let Some(err) = stderr {
            let (shared, job, done) = (self.clone(), job.clone(), done_tx.clone());
            spawn_thread(format!("epoch-job-{}-stderr", job.id.0), move || {
                read_lines(err, |line| shared.on_line(&job, JobEvent::Stderr { line }));
                let _ = done.send(());
            });
        }
        drop(done_tx);
        let (shared, job) = (self.clone(), job.clone());
        spawn_thread(format!("epoch-job-{}-wait", job.id.0), move || {
            shared.wait_for_exit(&job, &process, done_rx);
        });
        Ok(())
    }

    fn on_line(&self, job: &Job, event: JobEvent) {
        let mut inner = job.lock();
        inner.output_lines += 1;
        inner.push(event, self.opts.log_capacity);
    }

    fn on_stdout(&self, job: &Job, line: String) {
        let event = classify_stdout(&line, job.plan.speaks_protocol);
        let JobEvent::Protocol { event: proto } = &event else {
            return self.on_line(job, event);
        };
        // The observer hears of a new run before anyone polling can see it,
        // so what it does (indexing the run) is done by then.
        let started = match proto {
            ProtocolEvent::RunStarted { run, dir, .. } => {
                let run_dir = dir
                    .as_deref()
                    .filter(|d| !d.is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| nexis_ml::runs_dir(&job.plan.cwd).join(run));
                self.notify(
                    job,
                    JobNotice::RunStarted {
                        run_id: run.clone(),
                        run_dir: run_dir.clone(),
                    },
                );
                Some(run_dir)
            }
            _ => None,
        };
        {
            let mut inner = job.lock();
            let p = &mut inner.progress;
            match proto {
                ProtocolEvent::RunStarted {
                    run,
                    total_epochs,
                    device,
                    ..
                } => {
                    p.run_id = Some(run.clone());
                    p.run_dir = started;
                    p.total_epochs = *total_epochs;
                    p.device.clone_from(device);
                }
                ProtocolEvent::Epoch { epoch, of, .. } => {
                    p.epoch = Some(*epoch);
                    if of.is_some() {
                        p.total_epochs = *of;
                    }
                }
                ProtocolEvent::Metric { epoch, .. } => {
                    p.metric_events += 1;
                    if epoch.is_some() && *epoch > p.epoch {
                        p.epoch = *epoch;
                    }
                }
                ProtocolEvent::RunFinished { status, .. } => p.outcome = Some(status.clone()),
                _ => {}
            }
            inner.push(event, self.opts.log_capacity);
        }
    }

    fn wait_for_exit(
        self: &Arc<Self>,
        job: &Arc<Job>,
        process: &Process,
        output_done: mpsc::Receiver<()>,
    ) {
        let exit = loop {
            match lock(&process.child).try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {}
                Err(e) => break Err(e),
            }
            std::thread::sleep(EXIT_POLL);
        };
        // The job is over: end anything it started that is still running
        // (and still holding our pipes open). Done straight after the exit
        // is reaped, so a process group id can't have been reused yet.
        process.group.kill();
        let deadline = Instant::now() + DRAIN_WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match output_done.recv_timeout(left) {
                Ok(()) => continue,
                Err(_) => break, // both readers done, or out of time
            }
        }
        let forced = job.lock().stop;
        let status = match (exit, forced) {
            (_, Some(forced)) => JobStatus::Cancelled { forced },
            (Ok(status), None) => JobStatus::Exited {
                code: status.code(),
            },
            (Err(e), None) => JobStatus::Failed {
                message: format!("lost track of the process: {e}"),
            },
        };
        self.finish(job, status);
    }

    /// Finish a job that had a process (or failed to get one). The
    /// observer hears first, as with a new run.
    fn finish(self: &Arc<Self>, job: &Arc<Job>, status: JobStatus) {
        self.notify(job, JobNotice::Finished(status.clone()));
        self.complete(&mut job.lock(), job, status);
        self.prune();
        self.pump();
    }

    /// The locked half of finishing: record the final status.
    fn complete(&self, inner: &mut JobInner, job: &Job, status: JobStatus) {
        inner.stdin = None;
        inner.process = None;
        inner.finished_at_ms = Some(now_ms());
        inner.set_status(status, self.opts.log_capacity);
        job.changed.notify_all();
    }

    /// For a job that never ran: tell the observer, tidy up.
    fn after_finish(self: &Arc<Self>, job: &Arc<Job>, status: JobStatus) {
        self.notify(job, JobNotice::Finished(status));
        self.prune();
        self.pump();
    }

    fn write_control(inner: &mut JobInner, message: Control) -> bool {
        let Some(stdin) = inner.stdin.as_mut() else {
            return false;
        };
        let ok = stdin
            .write_all(message.to_line().as_bytes())
            .and_then(|()| stdin.flush())
            .is_ok();
        if !ok {
            inner.stdin = None;
        }
        ok
    }

    fn stop(self: &Arc<Self>, id: JobId, mode: StopMode) -> Result<(), RuntimeError> {
        let job = self.job(id)?;
        let mut inner = job.lock();
        let cap = self.opts.log_capacity;
        match inner.status.clone() {
            JobStatus::Queued => {
                // Under the same lock as the check, so the pump can't start
                // it in between.
                let status = JobStatus::Cancelled { forced: false };
                self.complete(&mut inner, &job, status.clone());
                drop(inner);
                self.after_finish(&job, status);
                return Ok(());
            }
            // `launch` sees the flag once the process exists.
            JobStatus::Starting => {
                inner.stop = Some(false);
                return Ok(());
            }
            status if status.is_finished() => return Err(RuntimeError::Finished(id)),
            JobStatus::Stopping { .. } if mode != StopMode::Kill => return Ok(()),
            _ => {}
        }
        let pid = inner.status.pid().unwrap_or_default();
        let Some(process) = inner.process.clone() else {
            return Err(RuntimeError::Finished(id));
        };
        if !matches!(inner.status, JobStatus::Stopping { .. }) {
            inner.set_status(JobStatus::Stopping { pid }, cap);
        }
        let timeout = match mode {
            StopMode::Kill => None,
            StopMode::Graceful { timeout } => {
                let asked = if job.plan.speaks_protocol {
                    Self::write_control(&mut inner, Control::Cancel)
                } else {
                    process.group.interrupt()
                };
                asked.then_some(timeout)
            }
        };
        match timeout {
            None => {
                inner.stop = Some(true);
                drop(inner);
                process.kill();
            }
            Some(timeout) => {
                inner.stop.get_or_insert(false);
                drop(inner);
                let job = job.clone();
                spawn_thread(format!("epoch-job-{}-stop", id.0), move || {
                    let deadline = Instant::now() + timeout;
                    let mut inner = job.lock();
                    while !inner.status.is_finished() {
                        let left = deadline.saturating_duration_since(Instant::now());
                        if left.is_zero() {
                            inner.stop = Some(true);
                            drop(inner);
                            process.kill();
                            return;
                        }
                        inner = job
                            .changed
                            .wait_timeout(inner, left)
                            .unwrap_or_else(|e| e.into_inner())
                            .0;
                    }
                });
            }
        }
        Ok(())
    }

    fn control(self: &Arc<Self>, id: JobId, message: Control) -> Result<(), RuntimeError> {
        if message == Control::Cancel {
            return self.stop(
                id,
                StopMode::Graceful {
                    timeout: DEFAULT_STOP_TIMEOUT,
                },
            );
        }
        let job = self.job(id)?;
        if !job.plan.speaks_protocol {
            return Err(RuntimeError::NotControllable(id));
        }
        let mut inner = job.lock();
        let next = match (&inner.status, message) {
            (JobStatus::Running { pid }, Control::Pause) => JobStatus::Paused { pid: *pid },
            (JobStatus::Paused { pid }, Control::Resume) => JobStatus::Running { pid: *pid },
            // Already in the requested state.
            (JobStatus::Paused { .. }, Control::Pause)
            | (JobStatus::Running { .. }, Control::Resume) => return Ok(()),
            (status, _) if status.is_finished() => return Err(RuntimeError::Finished(id)),
            _ => return Err(RuntimeError::NotControllable(id)),
        };
        if !Self::write_control(&mut inner, message) {
            return Err(RuntimeError::Io("the process's input is closed".into()));
        }
        inner.set_status(next, self.opts.log_capacity);
        Ok(())
    }
}

fn spawn_thread(name: String, f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .name(name)
        .spawn(f)
        .expect("the OS refused a thread");
}

impl Runtime for LocalRuntime {
    fn start(&self, spec: JobSpec) -> JobId {
        let id = JobId(self.shared.next_id.fetch_add(1, Ordering::Relaxed));
        let plan = spec.plan();
        let mut inner = JobInner {
            status: JobStatus::Queued,
            started_at_ms: None,
            finished_at_ms: None,
            records: VecDeque::new(),
            next_seq: 1,
            progress: JobProgress::default(),
            output_lines: 0,
            stdin: None,
            process: None,
            stop: None,
        };
        inner.push(
            JobEvent::Status(JobStatus::Queued),
            self.shared.opts.log_capacity,
        );
        let job = Arc::new(Job {
            id,
            spec,
            plan,
            queued_at_ms: now_ms(),
            inner: Mutex::new(inner),
            changed: Condvar::new(),
        });
        lock(&self.shared.jobs).insert(id, job);
        self.shared.pump();
        id
    }

    fn control(&self, id: JobId, message: Control) -> Result<(), RuntimeError> {
        self.shared.control(id, message)
    }

    fn stop(&self, id: JobId, mode: StopMode) -> Result<(), RuntimeError> {
        self.shared.stop(id, mode)
    }

    fn status(&self, id: JobId) -> Result<JobStatus, RuntimeError> {
        Ok(self.shared.job(id)?.lock().status.clone())
    }

    /// Newest first.
    fn jobs(&self) -> Vec<JobInfo> {
        let jobs: Vec<Arc<Job>> = lock(&self.shared.jobs).values().cloned().collect();
        jobs.iter().rev().map(|j| j.info()).collect()
    }

    fn events(&self, id: JobId, after: u64, max: usize) -> Result<EventPage, RuntimeError> {
        let job = self.shared.job(id)?;
        let inner = job.lock();
        let first = inner.records.front().map_or(inner.next_seq, |r| r.seq);
        let dropped = first.saturating_sub(after.saturating_add(1));
        let records: Vec<JobRecord> = inner
            .records
            .iter()
            .filter(|r| r.seq > after)
            .take(max.max(1))
            .cloned()
            .collect();
        let next = records
            .last()
            .map_or(after.max(first.saturating_sub(1)), |r| r.seq);
        let done = inner.status.is_finished() && next + 1 >= inner.next_seq;
        Ok(EventPage {
            records,
            next,
            dropped,
            done,
        })
    }
}
