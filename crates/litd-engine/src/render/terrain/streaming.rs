//! Chunked-clipmap streaming: a bounded node atlas plus a deterministic stream
//! request queue, following the `bevy_terrain` residency + coarse-fallback model.
//!
//! Tile heightmap data is content-addressed and streamed into a fixed-capacity
//! atlas. When the selector wants a tile whose data has not arrived yet, the
//! runtime draws the nearest resident ANCESTOR (a coarser tile that always covers
//! the same ground) so the frame is hole-free while streaming catches up. The
//! root tile is pinned resident, guaranteeing a fallback always exists.

use std::collections::HashSet;

use super::quadtree::NodeId;

/// A single deferred tile-load request emitted by selection, ordered by priority
/// (lower `distance_sq` first) so the nearest missing tiles stream in first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamRequest {
    pub id: NodeId,
    pub distance_sq: f32,
}

/// Fixed-capacity residency set for streamed tile data. Deterministic: no
/// wall-clock, no async; residency is mutated only by explicit stream calls so a
/// replay reproduces the exact same fallback decisions (valve #725).
#[derive(Debug, Clone)]
pub struct NodeAtlas {
    capacity: usize,
    resident: HashSet<NodeId>,
    root: NodeId,
    pending: Vec<StreamRequest>,
}

impl NodeAtlas {
    /// Create an atlas of `capacity` tiles with `root` pinned resident.
    pub fn new(capacity: usize, root: NodeId) -> Self {
        let mut resident = HashSet::new();
        resident.insert(root);
        Self {
            capacity: capacity.max(1),
            resident,
            root,
            pending: Vec::new(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn resident_count(&self) -> usize {
        self.resident.len()
    }

    pub fn is_resident(&self, id: NodeId) -> bool {
        self.resident.contains(&id)
    }

    /// Stream a tile in. Returns false (and does not evict) if the atlas is full
    /// and the tile is not the root -- the caller keeps using coarse fallback.
    pub fn mark_resident(&mut self, id: NodeId) -> bool {
        if self.resident.contains(&id) {
            return true;
        }
        if self.resident.len() >= self.capacity {
            return false;
        }
        self.resident.insert(id);
        true
    }

    /// Walk toward the root until a resident node is found. The pinned root
    /// guarantees this terminates. Returns `(drawn_id, was_fallback)`.
    pub fn resident_or_coarser(&self, id: NodeId) -> (NodeId, bool) {
        if self.resident.contains(&id) {
            return (id, false);
        }
        let mut cur = id;
        while cur.lod < self.root.lod {
            cur = NodeId::new(cur.lod + 1, cur.x / 2, cur.z / 2);
            if self.resident.contains(&cur) {
                return (cur, true);
            }
        }
        // Root is always resident.
        (self.root, true)
    }

    /// Queue a load request for a non-resident tile (deduped by the caller's
    /// per-frame visit; priority ordering happens in `drain_requests`).
    pub fn request(&mut self, id: NodeId, distance_sq: f32) {
        if self.resident.contains(&id) {
            return;
        }
        self.pending.push(StreamRequest { id, distance_sq });
    }

    /// Take the accumulated requests, nearest-first, deduped. Deterministic tie
    /// break on `NodeId` so identical frames yield an identical load order.
    pub fn drain_requests(&mut self) -> Vec<StreamRequest> {
        let mut reqs = std::mem::take(&mut self.pending);
        reqs.sort_by(|a, b| {
            a.distance_sq
                .total_cmp(&b.distance_sq)
                .then((a.id.lod, a.id.z, a.id.x).cmp(&(b.id.lod, b.id.z, b.id.x)))
        });
        reqs.dedup_by_key(|r| r.id);
        reqs
    }
}
