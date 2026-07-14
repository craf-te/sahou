//! sahou gui (design §1 option A): a thin Rust backend + the in-browser core wasm.
//! The backend only bridges raw bytes — the single place that interprets the contract's meaning is the in-browser core wasm.
pub mod files;
mod serve;

use std::path::PathBuf;

use clap::Args;
use sahou_core::diag::Diag;

#[derive(Args)]
pub struct GuiArgs {
    /// Target directory (when omitted = cwd). Handles schema.sahou.yaml / layout.sahou.json / endpoints.<env>.yaml
    pub path: Option<PathBuf>,
    /// The endpoints environment name (endpoints.<env>.yaml; default dev)
    #[arg(long, default_value = "dev")]
    pub env: String,
    /// Listen port (127.0.0.1 only). Rejects and stops if occupied (does not silently switch to another port)
    #[arg(long, default_value_t = 4649)]
    pub port: u16,
    /// Do not open a browser on startup. By default a dedicated app-mode window opens automatically
    /// (Chromium/Edge --app=<url>), so it also closes itself when the server stops (§1)
    #[arg(long = "no-open")]
    pub no_open: bool,
}

pub fn run(args: GuiArgs) -> Result<(), Vec<Diag>> {
    let dir = args.path.unwrap_or_else(|| PathBuf::from("."));
    let targets = files::resolve_targets(&dir, &args.env);
    let server = serve::start(targets.clone(), args.port)?;
    // watch -> SSE bridge: relay external mtime changes to the browser as {kind, etag} (§3.4)
    let (tx, rx) = std::sync::mpsc::channel();
    let _watcher = files::watch_targets(targets, tx).map_err(|e| {
        vec![Diag::new(
            "gui_watch_error",
            "$",
            format!("cannot start file watching: {e}"),
        )]
    })?;
    let hub = server.hub();
    std::thread::spawn(move || {
        for (kind, etag) in rx {
            hub.broadcast(&serde_json::json!({ "kind": kind.as_str(), "etag": etag }).to_string());
        }
    });
    let url = format!("http://{}", server.addr);
    println!("[ok] sahou gui: {url} (env={}; Ctrl+C to exit)", args.env);
    if !args.no_open {
        open_browser(&url);
    }
    server.serve_forever();
    Ok(())
}

/// Open the GUI. Prefer a Chromium/Edge app-mode window (`--app=<url>`): it is a dedicated, script-openable
/// window, so the page can close itself with `window.close()` when the server stops (§1). Fall back to a
/// normal browser open when no Chromium-family browser is found (a normal tab cannot be auto-closed by script;
/// the page then just shows a "server stopped" overlay).
fn open_browser(url: &str) {
    if open_app_mode(url) {
        return;
    }
    if let Err(e) = open_normal(url) {
        eprintln!("[warn] cannot open a browser: {e} (open the URL manually: {url})");
    }
}

#[cfg(target_os = "windows")]
fn open_app_mode(url: &str) -> bool {
    let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".to_string());
    let pf86 = std::env::var("ProgramFiles(x86)")
        .unwrap_or_else(|_| r"C:\Program Files (x86)".to_string());
    let candidates = [
        format!(r"{pf}\Google\Chrome\Application\chrome.exe"),
        format!(r"{pf86}\Google\Chrome\Application\chrome.exe"),
        format!(r"{pf86}\Microsoft\Edge\Application\msedge.exe"),
        format!(r"{pf}\Microsoft\Edge\Application\msedge.exe"),
    ];
    candidates.iter().any(|exe| {
        std::path::Path::new(exe).exists()
            && std::process::Command::new(exe)
                .arg(format!("--app={url}"))
                .spawn()
                .is_ok()
    })
}

#[cfg(target_os = "windows")]
fn open_normal(url: &str) -> std::io::Result<std::process::Child> {
    std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
}

#[cfg(target_os = "macos")]
fn open_app_mode(url: &str) -> bool {
    ["Google Chrome", "Microsoft Edge", "Chromium"]
        .iter()
        .any(|app| {
            std::process::Command::new("open")
                .args(["-na", app, "--args", &format!("--app={url}")])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        })
}

#[cfg(target_os = "macos")]
fn open_normal(url: &str) -> std::io::Result<std::process::Child> {
    std::process::Command::new("open").arg(url).spawn()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_app_mode(url: &str) -> bool {
    [
        "google-chrome",
        "chromium",
        "chromium-browser",
        "microsoft-edge",
    ]
    .iter()
    .any(|bin| {
        std::process::Command::new(bin)
            .arg(format!("--app={url}"))
            .spawn()
            .is_ok()
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_normal(url: &str) -> std::io::Result<std::process::Child> {
    std::process::Command::new("xdg-open").arg(url).spawn()
}
