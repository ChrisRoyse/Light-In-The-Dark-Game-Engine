//! Content-addressed on-disk store for frozen procedural meshes.
//!
//! A mesh is addressed by its `content_hash` (see [`super::mesh`]). Storing
//! writes the two opaque byte buffers plus a small JSON manifest under
//! `<root>/<content_hash>/`; loading reads them back and re-verifies every
//! digest before returning. Per the Calyx side-store doctrine (#1348) the only
//! bytes on disk here are the content-addressed opaque geometry blobs — their
//! metadata still flows through Calyx when ingested by the editor/MCP layer.

use std::fs;
use std::path::{Path, PathBuf};

use litd_hash::is_sha256_hex;

use crate::EngineError;

use super::mesh::FrozenMesh;

const VERTEX_FILE: &str = "vertices.bin";
const INDEX_FILE: &str = "indices.bin";
const MANIFEST_FILE: &str = "manifest.json";

/// A reference to a mesh persisted in the store.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredMeshRef {
    pub content_hash: String,
    pub manifest_path: PathBuf,
    pub vertex_path: PathBuf,
    pub index_path: PathBuf,
    pub vertex_bytes_len: u64,
    pub index_bytes_len: u64,
}

/// A filesystem-backed content-addressed mesh store.
#[derive(Debug, Clone)]
pub struct ProcGenStore {
    root: PathBuf,
}

impl ProcGenStore {
    /// Open (creating if needed) a store rooted at `root`.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, EngineError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(|error| persistence("open", &root, error))?;
        Ok(Self { root })
    }

    /// Persist a frozen mesh. The mesh is verified before any bytes are written,
    /// so a corrupt in-memory mesh cannot poison the store.
    pub fn store_mesh(&self, mesh: &FrozenMesh) -> Result<StoredMeshRef, EngineError> {
        mesh.verify()?;
        if !is_sha256_hex(&mesh.content_hash) {
            return Err(EngineError::ProcGenPersistenceFailed {
                op: "store".to_string(),
                reason: format!(
                    "content_hash {} is not a canonical digest",
                    mesh.content_hash
                ),
            });
        }
        let dir = self.root.join(&mesh.content_hash);
        fs::create_dir_all(&dir).map_err(|error| persistence("store.mkdir", &dir, error))?;
        let vertex_path = dir.join(VERTEX_FILE);
        let index_path = dir.join(INDEX_FILE);
        let manifest_path = dir.join(MANIFEST_FILE);
        fs::write(&vertex_path, &mesh.vertex_bytes)
            .map_err(|error| persistence("store.vertices", &vertex_path, error))?;
        fs::write(&index_path, &mesh.index_bytes)
            .map_err(|error| persistence("store.indices", &index_path, error))?;
        let manifest = serde_json::to_vec_pretty(mesh).map_err(|error| {
            EngineError::ProcGenPersistenceFailed {
                op: "store.manifest.encode".to_string(),
                reason: error.to_string(),
            }
        })?;
        fs::write(&manifest_path, &manifest)
            .map_err(|error| persistence("store.manifest", &manifest_path, error))?;
        Ok(StoredMeshRef {
            content_hash: mesh.content_hash.clone(),
            manifest_path,
            vertex_path,
            index_path,
            vertex_bytes_len: mesh.vertex_bytes.len() as u64,
            index_bytes_len: mesh.index_bytes.len() as u64,
        })
    }

    /// Read a mesh back by its content hash and re-verify every digest.
    pub fn load_mesh(&self, content_hash: &str) -> Result<FrozenMesh, EngineError> {
        if !is_sha256_hex(content_hash) {
            return Err(EngineError::ProcGenPersistenceFailed {
                op: "load".to_string(),
                reason: format!("content_hash {content_hash} is not a canonical digest"),
            });
        }
        let dir = self.root.join(content_hash);
        let manifest_path = dir.join(MANIFEST_FILE);
        let manifest_bytes = fs::read(&manifest_path)
            .map_err(|error| persistence("load.manifest", &manifest_path, error))?;
        let mut mesh: FrozenMesh = serde_json::from_slice(&manifest_bytes).map_err(|error| {
            EngineError::ProcGenPersistenceFailed {
                op: "load.manifest.decode".to_string(),
                reason: error.to_string(),
            }
        })?;
        let vertex_path = dir.join(VERTEX_FILE);
        let index_path = dir.join(INDEX_FILE);
        mesh.vertex_bytes = fs::read(&vertex_path)
            .map_err(|error| persistence("load.vertices", &vertex_path, error))?;
        mesh.index_bytes = fs::read(&index_path)
            .map_err(|error| persistence("load.indices", &index_path, error))?;
        // Independent readback proof: the on-disk bytes re-hash to the digests
        // carried in the manifest, and the recomputed content address matches
        // the directory we were asked to load.
        mesh.verify()?;
        if mesh.content_hash != content_hash {
            return Err(EngineError::ProcGenReadbackMismatch {
                field: "store.content_hash".to_string(),
                expected: content_hash.to_string(),
                actual: mesh.content_hash,
            });
        }
        Ok(mesh)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn persistence(op: &str, path: &Path, error: std::io::Error) -> EngineError {
    EngineError::ProcGenPersistenceFailed {
        op: op.to_string(),
        reason: format!("{}: {error}", path.display()),
    }
}
