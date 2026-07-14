//! C ABI (feature = "capi"): the C / C++ / Go / TouchDesigner entry point into the pure core.
//! Same "string in / JSON envelope out" contract as wasm.rs, so diagnostics are byte-identical
//! across every language. cbindgen renders these `extern "C"` functions into `sahou.h`.

use std::ffi::{c_char, CStr, CString};

use crate::ffi;
use crate::ir::Descriptor;
use crate::parse::parse_contract;
use crate::runtime as rt;
use crate::sample::sample_slot;
use crate::schema_check::validate_schema;

/// Envelope used when we cannot even run (null/invalid input, or a panic). Shaped like a normal NG.
fn internal_error(msg: &str) -> String {
    serde_json::json!({
        "ok": false,
        "diags": [{ "code": "capi_error", "path": "$", "message": msg }]
    })
    .to_string()
}

/// Borrow a C string as `&str`. `None` for a null pointer or invalid UTF-8.
///
/// # Safety
/// `ptr` must be null or a valid NUL-terminated C string that outlives the call.
unsafe fn cstr<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok()
}

/// Hand a String to the C side as a heap C string (release it with `sahou_free`).
fn into_c(s: String) -> *mut c_char {
    CString::new(s)
        .unwrap_or_else(|_| CString::new(internal_error("output contained a NUL byte")).unwrap())
        .into_raw()
}

/// Validate a contract (`schema.sahou.yaml` text). Returns a heap JSON string
/// `{"ok":true,"diags":[]}` | `{"ok":false,"diags":[{code,path,message},...]}`.
/// The caller must release the result with `sahou_free`.
///
/// # Safety
/// `yaml` must be null or a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sahou_validate_schema(yaml: *const c_char) -> *mut c_char {
    // A panic must never cross the FFI boundary (that is UB), so contain it here.
    let out = std::panic::catch_unwind(|| {
        let Some(yaml) = (unsafe { cstr(yaml) }) else {
            return internal_error("yaml pointer was null or not valid UTF-8");
        };
        let diags = match parse_contract(yaml) {
            Ok(c) => validate_schema(&c),
            Err(diags) => diags,
        };
        serde_json::json!({ "ok": diags.is_empty(), "diags": diags }).to_string()
    })
    .unwrap_or_else(|_| internal_error("internal panic while validating"));
    into_c(out)
}

/// Free a string returned by any `sahou_*` function. Passing null is a no-op.
///
/// # Safety
/// `s` must be null or a pointer previously returned by a `sahou_*` function and not yet freed.
#[no_mangle]
pub unsafe extern "C" fn sahou_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// Opaque runtime handle over a loaded descriptor. Wraps the same core the wasm/PyO3 runtimes use.
pub struct SahouRuntime {
    desc: Descriptor,
}

/// Load a descriptor (the text of `gen/descriptor.json`) into a runtime handle. Returns null if the
/// descriptor is invalid. Release with `sahou_runtime_free`.
///
/// # Safety
/// `descriptor_json` must be null or a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sahou_runtime_new(descriptor_json: *const c_char) -> *mut SahouRuntime {
    std::panic::catch_unwind(|| {
        let Some(json) = (unsafe { cstr(descriptor_json) }) else {
            return std::ptr::null_mut();
        };
        match rt::load_descriptor(json) {
            Ok(desc) => Box::into_raw(Box::new(SahouRuntime { desc })),
            Err(_) => std::ptr::null_mut(),
        }
    })
    .unwrap_or(std::ptr::null_mut())
}

/// Free a runtime handle from `sahou_runtime_new`. Passing null is a no-op.
///
/// # Safety
/// `handle` must be null or a handle from `sahou_runtime_new` that has not been freed.
#[no_mangle]
pub unsafe extern "C" fn sahou_runtime_free(handle: *mut SahouRuntime) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// List the nodes that can publish (are the `from` of a pub_sub connection), for a sender-node
/// selector. Returns a heap JSON string array `["node",...]` (empty on a null handle / panic).
/// Free with `sahou_free`.
///
/// # Safety
/// `handle` is null or a live runtime from `sahou_runtime_new`.
#[no_mangle]
pub unsafe extern "C" fn sahou_node_list(handle: *mut SahouRuntime) -> *mut c_char {
    let out = std::panic::catch_unwind(|| {
        let Some(rt_ref) = (unsafe { handle.as_ref() }) else {
            return "[]".to_string();
        };
        serde_json::to_string(&rt::publishing_nodes(&rt_ref.desc))
            .unwrap_or_else(|_| "[]".to_string())
    })
    .unwrap_or_else(|_| "[]".to_string());
    into_c(out)
}

/// List the pub_sub connections `node` can publish on, for a connection selector. Returns a heap
/// JSON string array `["conn",...]` (empty for a null handle / unknown node). Free with `sahou_free`.
///
/// # Safety
/// `handle` is null or a live runtime; `node` is null or a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sahou_connections_from(
    handle: *mut SahouRuntime,
    node: *const c_char,
) -> *mut c_char {
    let out = std::panic::catch_unwind(|| {
        let (Some(rt_ref), Some(node)) = (unsafe { handle.as_ref() }, unsafe { cstr(node) }) else {
            return "[]".to_string();
        };
        serde_json::to_string(&rt::connections_from(&rt_ref.desc, node))
            .unwrap_or_else(|_| "[]".to_string())
    })
    .unwrap_or_else(|_| "[]".to_string());
    into_c(out)
}

/// Payload schema of `conn` as display rows, for a "what should I send?" panel. Returns a heap JSON
/// array of `[name, type, required, detail]` string rows: `[["x","float","yes","0..1"],...]`
/// (empty array for a null handle / unknown / any-typed connection). Free with `sahou_free`.
///
/// # Safety
/// `handle` is null or a live runtime; `conn` is null or a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sahou_connection_fields(
    handle: *mut SahouRuntime,
    conn: *const c_char,
) -> *mut c_char {
    let out = std::panic::catch_unwind(|| {
        let (Some(rt_ref), Some(conn)) = (unsafe { handle.as_ref() }, unsafe { cstr(conn) }) else {
            return "[]".to_string();
        };
        serde_json::to_string(&rt::connection_fields(&rt_ref.desc, conn))
            .unwrap_or_else(|_| "[]".to_string())
    })
    .unwrap_or_else(|_| "[]".to_string());
    into_c(out)
}

/// Generate a valid sample payload for `conn`'s pub_sub payload (IR-typed), for a "Test send".
/// Returns a heap JSON object string (`{}` for a null handle / unknown / any-typed connection).
/// Deterministic (the core has no randomness). Free with `sahou_free`.
///
/// # Safety
/// `handle` is null or a live runtime; `conn` is null or a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sahou_sample(
    handle: *mut SahouRuntime,
    conn: *const c_char,
) -> *mut c_char {
    let out = std::panic::catch_unwind(|| {
        let (Some(rt_ref), Some(conn)) = (unsafe { handle.as_ref() }, unsafe { cstr(conn) }) else {
            return "{}".to_string();
        };
        match rt::conn_of(&rt_ref.desc, conn) {
            Ok(c) => match &c.payload {
                Some(slot) => sample_slot(slot).to_string(),
                None => "{}".to_string(),
            },
            Err(_) => "{}".to_string(),
        }
    })
    .unwrap_or_else(|_| "{}".to_string());
    into_c(out)
}

/// pub_sub send boundary (TouchDesigner OUT). Returns
/// `{"ok":true,"msg":{key,wire,attachment,qos}}` | `{"ok":false,"diags":[...]}`. Free with `sahou_free`.
///
/// # Safety
/// `handle` is a live runtime; `node`/`conn`/`payload_json` are valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn sahou_prepare_publish(
    handle: *mut SahouRuntime,
    node: *const c_char,
    conn: *const c_char,
    payload_json: *const c_char,
    seq: u64,
) -> *mut c_char {
    let out = std::panic::catch_unwind(|| {
        let (Some(rt_ref), Some(node), Some(conn), Some(payload)) = (
            unsafe { handle.as_ref() },
            unsafe { cstr(node) },
            unsafe { cstr(conn) },
            unsafe { cstr(payload_json) },
        ) else {
            return internal_error("null pointer or invalid UTF-8");
        };
        ffi::publish_envelope(rt::prepare_publish(&rt_ref.desc, node, conn, payload, seq))
    })
    .unwrap_or_else(|_| internal_error("internal panic in prepare_publish"));
    into_c(out)
}

/// pub_sub receive boundary (TouchDesigner IN). `wire` = the payload bytes; `attachment` = the
/// 16-hex per-connection hash (or null); `trusted` = a sender_hash already accepted (or null).
/// Returns tagged AcceptOutcome JSON. Free with `sahou_free`.
///
/// # Safety
/// `handle` is a live runtime; `node`/`conn` are valid C strings; `attachment`/`trusted` are null
/// or valid C strings; `wire` points to `wire_len` readable bytes (or is null with `wire_len` 0).
#[no_mangle]
pub unsafe extern "C" fn sahou_accept_sample(
    handle: *mut SahouRuntime,
    node: *const c_char,
    conn: *const c_char,
    wire: *const u8,
    wire_len: usize,
    attachment: *const c_char,
    seq: u64,
    trusted: *const c_char,
) -> *mut c_char {
    let out = std::panic::catch_unwind(|| {
        let (Some(rt_ref), Some(node), Some(conn)) =
            (unsafe { handle.as_ref() }, unsafe { cstr(node) }, unsafe {
                cstr(conn)
            })
        else {
            return internal_error("null pointer or invalid UTF-8");
        };
        let wire_slice = if wire.is_null() {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(wire, wire_len) }
        };
        ffi::outcome_json(rt::accept_sample(
            &rt_ref.desc,
            node,
            conn,
            wire_slice,
            unsafe { cstr(attachment) },
            seq,
            unsafe { cstr(trusted) },
        ))
    })
    .unwrap_or_else(|_| internal_error("internal panic in accept_sample"));
    into_c(out)
}
