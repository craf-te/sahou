//! Sahou transport (zenoh) C ABI for the TouchDesigner plugin.
//!
//! The pure core (libsahou_core) decides *what* to send/accept (validate + assemble the wire/key,
//! or run the receive boundary). This crate only moves *bytes*: a background zenoh peer session
//! (async isolated on its own thread, per design D11) that publishes `(key, wire, attachment)`
//! messages, and declares subscribers that stash the latest received `(wire, attachment)` per key
//! for the op to poll. It stays a dumb pipe — it never calls the core. Mirrors the CLI `tap` path
//! (same zenoh 1.9), so a TD publish/subscribe and `sahou tap` interoperate.
//!
//! De-risk scope: default QoS, one background session, fire-and-forget publish, latest-wins receive
//! (bursts between cooks are coalesced). Per-frame QoS mapping and on-change dedup come later.

use std::any::Any;
use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use zenoh::{Session, Wait};

/// (keyexpr, wire bytes as UTF-8 JSON, per-connection attachment hash)
type Msg = (String, String, String);

static TX: OnceLock<Sender<Msg>> = OnceLock::new();
static OPENED: AtomicBool = AtomicBool::new(false);
static SENT: AtomicU64 = AtomicU64::new(0);
static LAST_ERROR: Mutex<String> = Mutex::new(String::new());

/// The opened peer session, shared so subscribers can be declared after start.
static SESSION: OnceLock<Arc<Session>> = OnceLock::new();

/// The latest sample received on a key (latest-wins). `generation` bumps on each new sample.
#[derive(Clone, Default)]
struct RawSample {
    generation: u64,
    wire: Vec<u8>,
    attachment: String,
}

/// A declared subscription: its latest slot, the live subscriber handle (dropping it undeclares),
/// and a refcount so multiple ops sharing a key share one subscriber.
struct SubEntry {
    latest: Arc<Mutex<RawSample>>,
    _sub: Box<dyn Any + Send>, // kept alive only to keep the subscriber declared
    refs: usize,
}

/// key -> subscription. `None` until the first subscribe (lazy init).
static SUBS: Mutex<Option<HashMap<String, SubEntry>>> = Mutex::new(None);

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
            // Share the session so subscribers can be declared from other threads (poll/subscribe).
            let session = Arc::new(session);
            let _ = SESSION.set(Arc::clone(&session));
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

/// Declare (ref-counted) a Zenoh subscriber for `key`, storing the latest sample. Idempotent per
/// key (repeated calls bump a refcount). Requires `sahou_transport_start` first.
///
/// # Safety
/// `key` is null or a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sahou_transport_subscribe(key: *const c_char) {
    let Some(key) = cstr_owned(key) else { return };
    let Some(session) = SESSION.get() else {
        set_error("transport not started (call sahou_transport_start first)");
        return;
    };
    let Ok(mut guard) = SUBS.lock() else { return };
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(entry) = map.get_mut(&key) {
        entry.refs += 1;
        return;
    }
    let latest = Arc::new(Mutex::new(RawSample::default()));
    let latest_cb = Arc::clone(&latest);
    let sub = match session
        .declare_subscriber(key.clone())
        .callback(move |sample: zenoh::sample::Sample| {
            let wire = sample.payload().to_bytes().to_vec();
            let attachment = sample
                .attachment()
                .and_then(|a| String::from_utf8(a.to_bytes().to_vec()).ok())
                .unwrap_or_default();
            if let Ok(mut slot) = latest_cb.lock() {
                slot.generation += 1;
                slot.wire = wire;
                slot.attachment = attachment;
            }
        })
        .wait()
    {
        Ok(s) => s,
        Err(e) => {
            set_error(format!("declare_subscriber failed: {e}"));
            return;
        }
    };
    map.insert(
        key,
        SubEntry {
            latest,
            _sub: Box::new(sub),
            refs: 1,
        },
    );
}

/// Return the latest sample for `key` if its generation is newer than `since_generation`, as JSON
/// `{"generation":N,"wire":"…","attachment":"…"}`; `"{}"` when nothing newer / unknown key.
/// Free with `sahou_transport_free`.
///
/// # Safety
/// `key` is null or a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sahou_transport_poll(
    key: *const c_char,
    since_generation: u64,
) -> *mut c_char {
    let none = || into_c("{}".to_string());
    let Some(key) = cstr_owned(key) else {
        return none();
    };
    let Ok(guard) = SUBS.lock() else {
        return none();
    };
    let Some(map) = guard.as_ref() else {
        return none();
    };
    let Some(entry) = map.get(&key) else {
        return none();
    };
    let Ok(slot) = entry.latest.lock() else {
        return none();
    };
    if slot.generation == 0 || slot.generation <= since_generation {
        return none();
    }
    let wire = String::from_utf8_lossy(&slot.wire).to_string();
    let json = serde_json::json!({
        "generation": slot.generation,
        "wire": wire,
        "attachment": slot.attachment,
    })
    .to_string();
    into_c(json)
}

/// Drop one subscription ref for `key`; undeclares the Zenoh subscriber at zero.
///
/// # Safety
/// `key` is null or a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sahou_transport_unsubscribe(key: *const c_char) {
    let Some(key) = cstr_owned(key) else { return };
    let Ok(mut guard) = SUBS.lock() else { return };
    let Some(map) = guard.as_mut() else { return };
    if let Some(entry) = map.get_mut(&key) {
        entry.refs = entry.refs.saturating_sub(1);
        if entry.refs == 0 {
            map.remove(&key); // dropping SubEntry drops the Subscriber -> undeclare
        }
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

/// Free a string returned by `sahou_transport_status` / `sahou_transport_poll`. Passing null is a
/// no-op.
///
/// # Safety
/// `s` is null or a pointer previously returned by this library.
#[no_mangle]
pub unsafe extern "C" fn sahou_transport_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}
