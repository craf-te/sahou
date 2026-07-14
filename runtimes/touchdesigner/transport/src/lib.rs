//! Sahou transport (zenoh) C ABI for the TouchDesigner plugin.
//!
//! The pure core (libsahou_core) decides *what* to send (validate + assemble the wire/key/qos).
//! This crate only does the *sending*: a background zenoh peer session (async isolated on its own
//! thread, per design D11) that publishes `(key, wire, attachment)` messages handed to it. Mirrors
//! the CLI `tap` publish path (same zenoh 1.9), so a TD publish and `sahou tap` interoperate.
//!
//! De-risk scope (Step 1): default QoS, one background session, fire-and-forget publish. Per-frame
//! send, QoS mapping, and on-change dedup come in the send stage.

use std::ffi::{c_char, CStr, CString};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use zenoh::Wait;

/// (keyexpr, wire bytes as UTF-8 JSON, per-connection attachment hash)
type Msg = (String, String, String);

static TX: OnceLock<Sender<Msg>> = OnceLock::new();
static OPENED: AtomicBool = AtomicBool::new(false);
static SENT: AtomicU64 = AtomicU64::new(0);
static LAST_ERROR: Mutex<String> = Mutex::new(String::new());

fn set_error(msg: impl Into<String>) {
    if let Ok(mut g) = LAST_ERROR.lock() {
        *g = msg.into();
    }
}

fn into_c(s: String) -> *mut c_char {
    CString::new(s)
        .unwrap_or_else(|_| CString::new("").unwrap())
        .into_raw()
}

/// # Safety
/// `p` is null or a valid NUL-terminated C string.
unsafe fn cstr_owned(p: *const c_char) -> Option<String> {
    if p.is_null() {
        None
    } else {
        CStr::from_ptr(p).to_str().ok().map(str::to_string)
    }
}

/// Start the background zenoh peer session (idempotent). `connect` = an optional explicit endpoint
/// like `tcp/127.0.0.1:7447` (also disables multicast scouting); null = default peer (LAN multicast
/// discovery). Safe to call every frame; only the first call starts the thread.
///
/// # Safety
/// `connect` is null or a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sahou_transport_start(connect: *const c_char) {
    let connect = cstr_owned(connect);
    TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Msg>();
        std::thread::spawn(move || {
            let mut config = zenoh::Config::default();
            let _ = config.insert_json5("mode", "\"peer\"");
            if let Some(ep) = &connect {
                let _ = config.insert_json5("scouting/multicast/enabled", "false");
                let _ = config.insert_json5("connect/endpoints", &format!("[\"{ep}\"]"));
            }
            let session = match zenoh::open(config).wait() {
                Ok(s) => {
                    OPENED.store(true, Ordering::Relaxed);
                    s
                }
                Err(e) => {
                    set_error(format!("zenoh open failed: {e}"));
                    return;
                }
            };
            // Let discovery/route convergence settle before the first put.
            std::thread::sleep(Duration::from_millis(200));
            while let Ok((key, wire, att)) = rx.recv() {
                match session
                    .put(&key, wire.into_bytes())
                    .attachment(att.into_bytes())
                    .wait()
                {
                    Ok(()) => {
                        SENT.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => set_error(format!("put failed: {e}")),
                }
            }
        });
        tx
    });
}

/// Queue one message to publish (non-blocking; the background thread does the actual put).
/// A no-op if `sahou_transport_start` has not been called or any pointer is null/invalid.
///
/// # Safety
/// `key`/`wire`/`attachment` are null or valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sahou_transport_publish(
    key: *const c_char,
    wire: *const c_char,
    attachment: *const c_char,
) {
    let (Some(k), Some(w), Some(a)) = (cstr_owned(key), cstr_owned(wire), cstr_owned(attachment))
    else {
        return;
    };
    match TX.get() {
        Some(tx) => {
            let _ = tx.send((k, w, a));
        }
        None => set_error("transport not started (call sahou_transport_start first)"),
    }
}

/// Status JSON `{"opened":bool,"sent":N,"error":"..."}` for the node to display. Free with
/// `sahou_transport_free`.
#[no_mangle]
pub extern "C" fn sahou_transport_status() -> *mut c_char {
    let error = LAST_ERROR.lock().map(|g| g.clone()).unwrap_or_default();
    let json = serde_json::json!({
        "opened": OPENED.load(Ordering::Relaxed),
        "sent": SENT.load(Ordering::Relaxed),
        "error": error,
    })
    .to_string();
    into_c(json)
}

/// Free a string returned by `sahou_transport_status`. Passing null is a no-op.
///
/// # Safety
/// `s` is null or a pointer previously returned by this library.
#[no_mangle]
pub unsafe extern "C" fn sahou_transport_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}
