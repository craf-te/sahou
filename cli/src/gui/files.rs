//! The file layer of sahou gui (design §2.1 files): resolving target paths, etag (mtime+size),
//! compare-and-swap atomic write, and notify watch.
//! The backend never parses the contract (raw bytes only; interpretation happens in the in-browser core wasm, §1).
//!
//! This layer is the public API that Task 7 (serve) consumes. Since it is wired into the HTTP handlers (serve.rs)
//! and the `sahou gui` subcommand (gui.rs), the temporary `#![allow(dead_code)]` is no longer needed.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::UNIX_EPOCH;

use notify::{RecursiveMode, Watcher};

/// The three editable files. Named the same as the API's {kind} (schema | layout | endpoints).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Schema,
    Layout,
    Endpoints,
}

impl FileKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "schema" => Some(Self::Schema),
            "layout" => Some(Self::Layout),
            "endpoints" => Some(Self::Endpoints),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Layout => "layout",
            Self::Endpoints => "endpoints",
        }
    }
}

/// Target paths (schema.sahou.yaml / layout.sahou.json / endpoints.<env>.yaml directly under the target dir; design §2.1).
#[derive(Debug, Clone)]
pub struct Targets {
    pub dir: PathBuf,
    pub env: String,
    pub schema: PathBuf,
    pub layout: PathBuf,
    pub endpoints: PathBuf,
}

pub fn resolve_targets(dir: &Path, env: &str) -> Targets {
    Targets {
        dir: dir.to_path_buf(),
        env: env.to_string(),
        schema: dir.join("schema.sahou.yaml"),
        layout: dir.join("layout.sahou.json"),
        endpoints: dir.join(format!("endpoints.{env}.yaml")),
    }
}

impl Targets {
    pub fn path_of(&self, kind: FileKind) -> &Path {
        match kind {
            FileKind::Schema => &self.schema,
            FileKind::Layout => &self.layout,
            FileKind::Endpoints => &self.endpoints,
        }
    }
    /// Map a watch event path -> which target it is (None if not a target; temp files etc. are ignored).
    pub fn kind_of(&self, path: &Path) -> Option<FileKind> {
        [FileKind::Schema, FileKind::Layout, FileKind::Endpoints]
            .into_iter()
            .find(|k| path.file_name() == self.path_of(*k).file_name())
    }
}

/// etag = "mtime_secs.mtime_nanos-size". A missing file yields Ok(None).
pub fn etag_of(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::metadata(path) {
        Ok(m) => {
            let mt = m.modified()?.duration_since(UNIX_EPOCH).unwrap_or_default();
            Ok(Some(format!(
                "{}.{}-{}",
                mt.as_secs(),
                mt.subsec_nanos(),
                m.len()
            )))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

#[derive(Debug)]
pub enum PutError {
    /// etag mismatch (conflict with an external edit). current = the current file's etag (None if absent).
    Conflict {
        current: Option<String>,
    },
    Io(String),
}

/// compare-and-swap atomic write (design §2.1). Writes temp + rename only when expected matches the current etag.
/// expected=None means "create new" (Conflict if one already exists). Return value = the new etag after writing.
/// There is a gap between stat and rename (not a perfect CAS), so in-process serialization is handled
/// by the serve layer's put_lock (sufficient for the single-user localhost use case; §2.1 "effectively stateless").
pub fn write_if_match(
    path: &Path,
    expected: Option<&str>,
    content: &str,
) -> Result<String, PutError> {
    let current = etag_of(path).map_err(|e| PutError::Io(e.to_string()))?;
    if current.as_deref() != expected {
        return Err(PutError::Conflict { current });
    }
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.sahou-gui.tmp"));
    std::fs::write(&tmp, content).map_err(|e| PutError::Io(e.to_string()))?;
    // Windows's std::fs::rename replaces an existing destination (MOVEFILE_REPLACE_EXISTING)
    std::fs::rename(&tmp, path).map_err(|e| PutError::Io(e.to_string()))?;
    etag_of(path)
        .map_err(|e| PutError::Io(e.to_string()))?
        .ok_or_else(|| PutError::Io("cannot stat immediately after writing".to_string()))
}

/// A watch that notifies external mtime changes as (FileKind, new etag). The returned watcher stops when dropped.
pub fn watch_targets(
    targets: Targets,
    tx: Sender<(FileKind, Option<String>)>,
) -> notify::Result<notify::RecommendedWatcher> {
    let dir = targets.dir.clone();
    let mut watcher = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
        let Ok(ev) = ev else { return };
        for p in &ev.paths {
            if let Some(kind) = targets.kind_of(p) {
                // The notification carries "the current etag" — the receiver (browser store) rejects self-echoes by comparing etags (§3.4)
                let _ = tx.send((kind, etag_of(targets.path_of(kind)).ok().flatten()));
            }
        }
    })?;
    watcher.watch(&dir, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_targets_uses_env_in_endpoints_name() {
        let t = resolve_targets(Path::new("x"), "prod");
        assert!(t.endpoints.ends_with("endpoints.prod.yaml"));
        assert!(t.schema.ends_with("schema.sahou.yaml"));
        assert_eq!(
            t.kind_of(Path::new("x/layout.sahou.json")),
            Some(FileKind::Layout)
        );
        assert_eq!(t.kind_of(Path::new("x/other.txt")), None);
    }

    #[test]
    fn etag_absent_then_changes_on_write() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("schema.sahou.yaml");
        assert_eq!(etag_of(&p).unwrap(), None);
        std::fs::write(&p, "a").unwrap();
        let e1 = etag_of(&p).unwrap().unwrap();
        std::fs::write(&p, "ab").unwrap();
        let e2 = etag_of(&p).unwrap().unwrap();
        assert_ne!(
            e1, e2,
            "size changes -> etag changes (does not depend on mtime resolution)"
        );
    }

    #[test]
    fn write_if_match_is_compare_and_swap() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("schema.sahou.yaml");
        // creating new is expected=None only
        let e1 = write_if_match(&p, None, "v1").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "v1");
        // None even though one exists -> Conflict (does not silently overwrite)
        assert!(matches!(
            write_if_match(&p, None, "x"),
            Err(PutError::Conflict { .. })
        ));
        // writing with a stale etag after an external edit -> Conflict + current etag (the input to the frontend's conflict resolution §5.3)
        std::fs::write(&p, "external").unwrap();
        match write_if_match(&p, Some(&e1), "mine").unwrap_err() {
            PutError::Conflict { current } => assert_eq!(current, etag_of(&p).unwrap()),
            PutError::Io(e) => panic!("not Io: {e}"),
        }
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "external",
            "the external edit is not clobbered"
        );
        // matching etag -> writable + new etag
        let cur = etag_of(&p).unwrap().unwrap();
        let e2 = write_if_match(&p, Some(&cur), "v2").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "v2");
        assert_ne!(e2, cur);
        // no temp file is left behind (atomic write cleanup)
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn watch_emits_kind_and_etag_on_external_change() {
        let dir = tempfile::tempdir().unwrap();
        let t = resolve_targets(dir.path(), "dev");
        std::fs::write(&t.schema, "schema: s\n").unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let _w = watch_targets(t.clone(), tx).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300)); // wait for the watcher to start
        std::fs::write(&t.schema, "schema: s2\n").unwrap();
        let (kind, etag) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("an external change emits an SSE-equivalent notification");
        assert_eq!(kind, FileKind::Schema);
        assert!(etag.is_some());
    }
}
