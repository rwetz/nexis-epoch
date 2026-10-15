//! The standalone Nexis Epoch app. Everything Epoch does is in
//! `tauri-plugin-epoch` (which Nexis's Epoch window registers too); this
//! crate adds only what belongs to a separate app: its window, the OS
//! plugins, and the loopback link channel to a running Nexis (links.rs).

mod links;

#[cfg(target_os = "linux")]
fn is_nvidia() -> bool {
    std::path::Path::new("/proc/driver/nvidia/version").exists()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    {
        // NVIDIA + Wayland: WebKitGTK's DMA-BUF renderer crashes at first
        // paint. Forcing Mesa's EGL keeps it alive *and* keeps window alpha
        // (the 12px transparent corners); disabling DMA-BUF is the fallback
        // that keeps it alive but paints the corners black.
        // @nexis/design docs/PITFALLS.md §2 — do not re-litigate.
        if is_nvidia()
            && std::env::var_os("APP_KEEP_HW_ACCEL").is_none()
            && std::env::var_os("__EGL_VENDOR_LIBRARY_FILENAMES").is_none()
        {
            const MESA: &str = "/usr/share/glvnd/egl_vendor.d/50_mesa.json";
            if std::path::Path::new(MESA).exists() {
                std::env::set_var("__EGL_VENDOR_LIBRARY_FILENAMES", MESA);
            } else {
                std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1"); // alive > pretty
            }
        }
        // KWin otherwise draws a server-side title bar over our own.
        if std::env::var_os("GTK_CSD").is_none() {
            std::env::set_var("GTK_CSD", "1");
        }
    }

    // A launch carrying a `nexis-epoch://` link (from Nexis) goes to the Epoch
    // that is already open and this process ends; otherwise Epoch opens and
    // follows the link once the window is up. Plain launches are unaffected.
    let launch_link = links::link_from_args(std::env::args().skip(1));
    if let Some(url) = &launch_link {
        if links::send("epoch", url).is_ok() {
            return;
        }
    }

    tauri::Builder::default()
        .manage(links::PendingLinks::default())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_epoch::init())
        .setup(move |app| {
            links::start_listener(app.handle().clone());
            if let Some(url) = &launch_link {
                links::queue_launch_link(app.handle(), url);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            links::links_take_pending,
            links::nexis_open,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Nexis Epoch");
}
