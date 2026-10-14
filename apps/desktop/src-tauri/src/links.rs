//! Links between Epoch and Nexis (Synapse, the ML lab, and Pulse, the
//! benchmark tool).
//!
//! Same small channel on both sides, no URL-scheme registration and no
//! single-instance plugin:
//!
//! * A running app listens on a loopback port and writes the port and a
//!   random token to `<local data dir>/nexis/links/<app>.json` (this app is
//!   `epoch`, Nexis is `nexis`). The folder is per user; the token keeps other
//!   local users, who can reach the port, from sending links.
//! * A sender reads the receiver's file, connects, and writes one JSON line
//!   `{"token": "...", "url": "..."}`. If nothing answers it launches the app
//!   with `--link <url>`; a launch carrying a link first tries the running
//!   copy and exits if it took it. Plain launches are unaffected.
//!
//! Inbound: `nexis-epoch://open?project=<dir>[&run=<id>][&dataset=<file>]`.
//! Outbound: `nexis://ml-lab?project=<dir>[&run=<id>]` and
//! `nexis://benchmark?model=<file>`. Nexis's half is `src-tauri/src/modules/
//! links.rs` in the Nexis repo; keep the two in step.

use serde::{Deserialize, Serialize};
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

pub const SCHEME: &str = "nexis-epoch";
pub const LINK_EVENT: &str = "epoch://link";

/// One validated inbound link: open a project, optionally focus a run or a
/// dataset in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpochLink {
    pub project: String,
    pub run: Option<String>,
    pub dataset: Option<String>,
}

pub fn parse(raw: &str) -> Option<EpochLink> {
    let url = tauri::Url::parse(raw.trim()).ok()?;
    if !url.scheme().eq_ignore_ascii_case(SCHEME) || !url.host_str()?.eq_ignore_ascii_case("open") {
        return None;
    }
    let (mut project, mut run, mut dataset) = (None, None, None);
    for (key, value) in url.query_pairs() {
        let value = value.into_owned();
        match key.as_ref() {
            "project" if is_safe_absolute(&value) => project = Some(value),
            "dataset" if is_safe_absolute(&value) => dataset = Some(value),
            "run" if is_run_id(&value) => run = Some(value),
            _ => return None,
        }
    }
    Some(EpochLink {
        project: project?,
        run,
        dataset,
    })
}

fn is_safe_absolute(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && value.len() < 4096
        && path.is_absolute()
        && !path.components().any(|c| matches!(c, Component::ParentDir))
}

fn is_run_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// `--link <url>`, `--link=<url>`, or a bare `nexis-epoch://` argument.
pub fn link_from_args<I: IntoIterator<Item = String>>(args: I) -> Option<String> {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--link" {
            return args.next();
        }
        if let Some(rest) = arg.strip_prefix("--link=") {
            return Some(rest.to_string());
        }
        if arg.to_ascii_lowercase().starts_with("nexis-epoch://") {
            return Some(arg);
        }
    }
    None
}

// ── the channel ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct Endpoint {
    port: u16,
    token: String,
    pid: u32,
}

fn endpoint_path(app: &str) -> Option<PathBuf> {
    Some(
        dirs::data_local_dir()?
            .join("nexis")
            .join("links")
            .join(format!("{app}.json")),
    )
}

fn random_token() -> String {
    // Two std hashers, each seeded from the OS RNG: 128 bits, no extra crate.
    let mut out = String::with_capacity(32);
    for salt in 0..2u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(salt);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default(),
        );
        h.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// Hand `url` to the running app `app` ("epoch" or "nexis").
pub fn send(app: &str, url: &str) -> Result<(), String> {
    let path = endpoint_path(app).ok_or("no local data dir")?;
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let ep: Endpoint = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let addr = SocketAddr::from(([127, 0, 0, 1], ep.port));
    let mut stream =
        TcpStream::connect_timeout(&addr, Duration::from_millis(600)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let line = serde_json::json!({ "token": ep.token, "url": url }).to_string();
    stream
        .write_all(format!("{line}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .map_err(|e| e.to_string())?;
    if reply.trim() == "ok" {
        Ok(())
    } else {
        Err(format!("refused: {}", reply.trim()))
    }
}

/// Links that arrived before the webview was listening.
#[derive(Default)]
pub struct PendingLinks(pub Mutex<Vec<EpochLink>>);

fn queue(app: &AppHandle, link: EpochLink) {
    if let Some(state) = app.try_state::<PendingLinks>() {
        state.0.lock().unwrap_or_else(|e| e.into_inner()).push(link);
    }
    let _ = app.emit(LINK_EVENT, ());
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

pub fn queue_launch_link(app: &AppHandle, url: &str) {
    match parse(url) {
        Some(link) => queue(app, link),
        None => eprintln!("[links] ignoring malformed launch link"),
    }
}

pub fn start_listener(app: AppHandle) {
    let Some(path) = endpoint_path("epoch") else {
        return;
    };
    let Ok(listener) = TcpListener::bind(("127.0.0.1", 0)) else {
        return;
    };
    let Ok(port) = listener.local_addr().map(|a| a.port()) else {
        return;
    };
    let token = random_token();
    let ep = Endpoint {
        port,
        token: token.clone(),
        pid: std::process::id(),
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    let written = serde_json::to_string(&ep)
        .ok()
        .and_then(|s| std::fs::write(&tmp, s).ok())
        .and_then(|_| std::fs::rename(&tmp, &path).ok());
    if written.is_none() {
        eprintln!("[links] could not write {}", path.display());
        return;
    }
    let _ = std::thread::Builder::new()
        .name("epoch-links".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                handle_conn(&app, stream, &token);
            }
        });
}

#[derive(Deserialize)]
struct Message {
    token: String,
    url: String,
}

fn handle_conn(app: &AppHandle, stream: TcpStream, token: &str) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut line = String::new();
    if BufReader::new(&stream).read_line(&mut line).is_err() || line.len() > 16 * 1024 {
        return;
    }
    let reply = match serde_json::from_str::<Message>(&line) {
        Ok(msg) if msg.token == token => match parse(&msg.url) {
            Some(link) => {
                queue(app, link);
                "ok"
            }
            None => "err bad-link",
        },
        Ok(_) => "err token",
        Err(_) => "err format",
    };
    let mut out = &stream;
    let _ = out.write_all(format!("{reply}\n").as_bytes());
}

#[tauri::command]
pub fn links_take_pending(state: State<'_, PendingLinks>) -> Vec<EpochLink> {
    std::mem::take(&mut *state.0.lock().unwrap_or_else(|e| e.into_inner()))
}

// ── to Nexis ────────────────────────────────────────────────────────────────

fn nexis_exe_name() -> &'static str {
    if cfg!(windows) {
        "nexis.exe"
    } else {
        "nexis"
    }
}

/// Where the Nexis installers put it, then anything called nexis on PATH.
fn locate_nexis() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(p) = std::env::var("NEXIS_EXE") {
        candidates.push(PathBuf::from(p));
    }
    if cfg!(windows) {
        if let Some(local) = dirs::data_local_dir() {
            candidates.push(local.join("Nexis").join(nexis_exe_name()));
            candidates.push(local.join("Programs").join("Nexis").join(nexis_exe_name()));
        }
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Ok(pf) = std::env::var(var) {
                candidates.push(PathBuf::from(pf).join("Nexis").join(nexis_exe_name()));
            }
        }
    } else if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from(
            "/Applications/Nexis.app/Contents/MacOS/nexis",
        ));
    } else {
        candidates.push(PathBuf::from("/usr/bin/nexis"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|d| d.join(nexis_exe_name())));
    }
    candidates.into_iter().find(|p| {
        p.is_file()
            && p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("nexis"))
    })
}

fn nexis_url(tool: &str, params: &[(&str, &str)]) -> Result<String, String> {
    if !matches!(tool, "ml-lab" | "benchmark") {
        return Err(format!("unknown Nexis tool: {tool}"));
    }
    let mut url = tauri::Url::parse(&format!("nexis://{tool}")).map_err(|e| e.to_string())?;
    {
        let mut q = url.query_pairs_mut();
        for (k, v) in params {
            q.append_pair(k, v);
        }
    }
    Ok(url.to_string())
}

/// Open Synapse on a project/run (`tool = "ml-lab"`) or Pulse on a model
/// (`tool = "benchmark"`). "sent" = a running Nexis took it; "launched" =
/// Epoch started Nexis; error "not-found" = no Nexis to start.
#[tauri::command]
pub async fn nexis_open(
    tool: String,
    project: Option<String>,
    run: Option<String>,
    model: Option<String>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut params: Vec<(&str, &str)> = Vec::new();
        for (key, value) in [("project", &project), ("model", &model)] {
            if let Some(v) = value {
                if !is_safe_absolute(v) {
                    return Err(format!("not an absolute path: {v}"));
                }
                params.push((key, v));
            }
        }
        if let Some(r) = &run {
            if !is_run_id(r) {
                return Err(format!("not a run id: {r}"));
            }
            params.push(("run", r));
        }
        let url = nexis_url(&tool, &params)?;
        if send("nexis", &url).is_ok() {
            return Ok("sent".to_string());
        }
        let exe = locate_nexis().ok_or_else(|| "not-found".to_string())?;
        let mut cmd = std::process::Command::new(&exe);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        cmd.arg("--link")
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("could not start {}: {e}", exe.display()))?;
        Ok("launched".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs(p: &str) -> String {
        if cfg!(windows) {
            format!("C:\\{}", p.replace('/', "\\"))
        } else {
            format!("/{p}")
        }
    }

    fn url(pairs: &[(&str, &str)]) -> String {
        let mut u = tauri::Url::parse("nexis-epoch://open").unwrap();
        for (k, v) in pairs {
            u.query_pairs_mut().append_pair(k, v);
        }
        u.to_string()
    }

    #[test]
    fn parses_project_run_and_dataset() {
        let project = abs("work/plant-doctor");
        let dataset = abs("work/plant-doctor/data/plants.csv");
        let link = parse(&url(&[
            ("project", &project),
            ("run", "2026-10-14-1737-tabular-2"),
            ("dataset", &dataset),
        ]))
        .unwrap();
        assert_eq!(link.project, project);
        assert_eq!(link.run.as_deref(), Some("2026-10-14-1737-tabular-2"));
        assert_eq!(link.dataset.as_deref(), Some(dataset.as_str()));
    }

    #[test]
    fn needs_a_project_and_rejects_the_rest() {
        assert!(parse(&url(&[("run", "r1")])).is_none());
        assert!(parse(&url(&[("project", "relative/dir")])).is_none());
        assert!(parse(&url(&[("project", &abs("a/../b"))])).is_none());
        assert!(parse(&url(&[("project", &abs("a")), ("exec", "x")])).is_none());
        assert!(parse(&url(&[("project", &abs("a")), ("run", "../r")])).is_none());
        assert!(parse("nexis-epoch://delete?project=/a").is_none());
        assert!(parse("nexis://open?project=/a").is_none());
    }

    #[test]
    fn finds_the_link_argument() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            link_from_args(a(&["--link", "nexis-epoch://open"])).as_deref(),
            Some("nexis-epoch://open")
        );
        assert_eq!(
            link_from_args(a(&["NEXIS-EPOCH://open?project=x"])).as_deref(),
            Some("NEXIS-EPOCH://open?project=x")
        );
        assert_eq!(link_from_args(a(&["--verbose"])), None);
    }

    #[test]
    fn builds_nexis_links_for_known_tools_only() {
        let u = nexis_url("ml-lab", &[("project", "C:\\p q"), ("run", "r1")]).unwrap();
        assert!(u.starts_with("nexis://ml-lab?project="));
        assert!(!u.contains(' '));
        assert!(nexis_url("terminal", &[]).is_err());
    }
}
