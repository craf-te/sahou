//! Python bindings (feature "python"). ABI = string/bytes in, JSON string out.
//! The Python glue (runtimes/python) imports this as `sahou._core`.
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::diag::Diag;
use crate::ffi;
use crate::ir::Descriptor;
use crate::runtime as rt;

fn diags_err(diags: Vec<Diag>) -> PyErr {
    PyValueError::new_err(ffi::diags_json(&diags))
}

#[pyclass]
pub struct SahouRuntime {
    desc: Descriptor,
}

#[pymethods]
impl SahouRuntime {
    #[new]
    fn new(descriptor_json: &str) -> PyResult<Self> {
        rt::load_descriptor(descriptor_json)
            .map(|desc| Self { desc })
            .map_err(diags_err)
    }

    fn namespace(&self) -> String {
        self.desc.namespace.clone()
    }

    fn node_plan(&self, node: &str) -> PyResult<String> {
        rt::node_plan(&self.desc, node)
            .map(|p| ffi::plan_json(&p))
            .map_err(diags_err)
    }

    fn prepare_publish(&self, node: &str, conn: &str, payload_json: &str, seq: u64) -> String {
        ffi::publish_envelope(rt::prepare_publish(
            &self.desc,
            node,
            conn,
            payload_json,
            seq,
        ))
    }

    #[pyo3(signature = (node, conn, wire, attachment=None, seq=0, trusted=None))]
    fn accept_sample(
        &self,
        node: &str,
        conn: &str,
        wire: &[u8],
        attachment: Option<&str>,
        seq: u64,
        trusted: Option<&str>,
    ) -> String {
        ffi::outcome_json(rt::accept_sample(
            &self.desc, node, conn, wire, attachment, seq, trusted,
        ))
    }

    fn prepare_request(&self, node: &str, conn: &str, payload_json: &str, seq: u64) -> String {
        ffi::publish_envelope(rt::prepare_request(
            &self.desc,
            node,
            conn,
            payload_json,
            seq,
        ))
    }

    #[pyo3(signature = (node, conn, wire, attachment=None, seq=0, trusted=None))]
    fn accept_request(
        &self,
        node: &str,
        conn: &str,
        wire: &[u8],
        attachment: Option<&str>,
        seq: u64,
        trusted: Option<&str>,
    ) -> String {
        ffi::outcome_json(rt::accept_request(
            &self.desc, node, conn, wire, attachment, seq, trusted,
        ))
    }

    fn prepare_reply(&self, node: &str, conn: &str, payload_json: &str, seq: u64) -> String {
        ffi::publish_envelope(rt::prepare_reply(&self.desc, node, conn, payload_json, seq))
    }

    #[pyo3(signature = (node, conn, wire, attachment=None, seq=0, trusted=None))]
    fn accept_reply(
        &self,
        node: &str,
        conn: &str,
        wire: &[u8],
        attachment: Option<&str>,
        seq: u64,
        trusted: Option<&str>,
    ) -> String {
        ffi::outcome_json(rt::accept_reply(
            &self.desc, node, conn, wire, attachment, seq, trusted,
        ))
    }

    fn contract_fragment(&self, conn: &str) -> PyResult<String> {
        rt::contract_fragment(&self.desc, conn).map_err(diags_err)
    }

    /// Build this node's vitals payload (vitals_format 1; spec: notes/sahou-vitals-spec.md).
    /// info_json = runtime facts (lang / sahou / zenoh / transport / uptime_secs / handshake).
    fn vitals_payload(&self, node: &str, info_json: &str) -> PyResult<String> {
        crate::vitals::vitals_payload(&self.desc, node, info_json).map_err(diags_err)
    }

    /// The key both the liveliness token and the vitals queryable use (one impl in the core).
    fn vitals_key(&self, node: &str) -> String {
        crate::vitals::vitals_key(&self.desc, node)
    }

    /// The 3-way handshake judgement. Always returns a verdict envelope (an unknown connection is an unreachable envelope = not an exception).
    fn handshake(&self, conn: &str, sender_hash: &str, theirs_json: &str) -> String {
        ffi::handshake_json(&rt::handshake_judge(
            &self.desc,
            conn,
            sender_hash,
            theirs_json,
        ))
    }
}

/// Classification for smart retry. diags_json = JSON array of Diag. Returns = "retryable" | "fatal"
#[pyfunction]
fn classify_delivery(timed_out: bool, diags_json: &str) -> PyResult<String> {
    let diags: Vec<Diag> = if diags_json.trim().is_empty() {
        vec![]
    } else {
        serde_json::from_str(diags_json)
            .map_err(|e| PyValueError::new_err(format!("invalid diags JSON: {e}")))?
    };
    Ok(ffi::delivery_str(rt::classify_delivery(timed_out, &diags)).to_string())
}

/// Parsing the reply_err envelope (a single core implementation). Returns = JSON array of Diag.
#[pyfunction]
fn parse_reply_err(payload: &[u8]) -> String {
    ffi::diags_json(&rt::parse_reply_err(payload))
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SahouRuntime>()?;
    m.add_function(wrap_pyfunction!(classify_delivery, m)?)?;
    m.add_function(wrap_pyfunction!(parse_reply_err, m)?)?;
    Ok(())
}
