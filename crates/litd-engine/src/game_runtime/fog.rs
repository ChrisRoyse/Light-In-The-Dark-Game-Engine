//! #847: deterministic SIM-SIDE fog of war / visibility.
//!
//! Visibility gates sim queries (targeting), so it MUST be reproducible under
//! lockstep replay (#85/#800): every computation here is integer math -- no
//! wall-clock, no float, no HashMap iteration order. Players and grids are held
//! in sorted vectors and the state hash folds cells in slot order, so identical
//! inputs always yield an identical [`FogState::state_hash`].
//!
//! Three per-player cell states (the WC3 model):
//! - `Masked`: never seen (the black mask) OR actively concealed.
//! - `Explored`: seen before, remembered terrain, not currently in sight.
//! - `Visible`: currently inside a friendly sight radius this recompute.
//!
//! Shared vision honors #836's alliance flags through the [`SharedVision`] trait
//! -- a minimal seam this track owns until #836's surface lands (see PR note).

use litd_hash::Fnv1a64;
use serde::{Deserialize, Serialize};

use crate::EngineError;

/// Player/force id a visibility grid belongs to.
pub type PlayerId = u32;

/// Per-cell visibility for one player. Ordinal is stable (used in the hash).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisibilityState {
    Masked = 0,
    Explored = 1,
    Visible = 2,
}

/// A unit's contribution to vision: its owner and an integer sight radius in
/// cells. `sight_radius_cells` is authored data lowered from the unit record
/// (the runtime hardcodes no radius -- doctrine #1312/#1783).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisionSource {
    pub owner: PlayerId,
    pub cell_x: i32,
    pub cell_y: i32,
    pub sight_radius_cells: u32,
}

/// Kind of a typed visibility modifier region (reveal/conceal via order/trigger).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModifierKind {
    /// Force every covered cell `Visible` for the player.
    Reveal,
    /// Force every covered cell `Masked` for the player (conceal wins over reveal).
    Conceal,
}

/// A rectangular typed visibility modifier (inclusive cell bounds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibilityModifier {
    pub player: PlayerId,
    pub kind: ModifierKind,
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

/// Alliance seam (#836). Returns whether `viewer` sees vision contributed by
/// `source_owner`. This track owns the trait until #836's surface exists.
pub trait SharedVision {
    fn shares_vision(&self, viewer: PlayerId, source_owner: PlayerId) -> bool;
}

/// Default: a player sees only their own units' vision.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSharedVision;

impl SharedVision for NoSharedVision {
    fn shares_vision(&self, viewer: PlayerId, source_owner: PlayerId) -> bool {
        viewer == source_owner
    }
}

/// A minimal symmetric alliance table standing in for #836's flags. A player
/// always shares vision with itself; listed unordered pairs share both ways.
#[derive(Debug, Clone, Default)]
pub struct SharedVisionTable {
    pairs: Vec<(PlayerId, PlayerId)>,
}

impl SharedVisionTable {
    pub fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Record a symmetric vision-sharing alliance between `a` and `b`.
    pub fn ally(&mut self, a: PlayerId, b: PlayerId) {
        let key = if a <= b { (a, b) } else { (b, a) };
        if !self.pairs.contains(&key) {
            self.pairs.push(key);
        }
    }
}

impl SharedVision for SharedVisionTable {
    fn shares_vision(&self, viewer: PlayerId, source_owner: PlayerId) -> bool {
        if viewer == source_owner {
            return true;
        }
        let key = if viewer <= source_owner {
            (viewer, source_owner)
        } else {
            (source_owner, viewer)
        };
        self.pairs.contains(&key)
    }
}

/// One player's visibility grid. Row-major, `width * height` cells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibilityGrid {
    pub width: u32,
    pub height: u32,
    cells: Vec<VisibilityState>,
}

impl VisibilityGrid {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            cells: vec![VisibilityState::Masked; (width * height) as usize],
        }
    }

    #[inline]
    fn index(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    /// Visibility state at an integer cell; out-of-bounds reads as `Masked`.
    pub fn state(&self, x: i32, y: i32) -> VisibilityState {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return VisibilityState::Masked;
        }
        self.cells[self.index(x as u32, y as u32)]
    }

    /// Borrow the row-major cells for presentation without copying sim state.
    pub fn states(&self) -> &[VisibilityState] {
        &self.cells
    }

    /// Count of cells in each state as `(masked, explored, visible)`.
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut out = (0usize, 0usize, 0usize);
        for cell in &self.cells {
            match cell {
                VisibilityState::Masked => out.0 += 1,
                VisibilityState::Explored => out.1 += 1,
                VisibilityState::Visible => out.2 += 1,
            }
        }
        out
    }
}

/// Per-player fog state for a map. Players are stored sorted+unique so iteration
/// (and thus the state hash) is order-independent of how they were supplied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FogState {
    width: u32,
    height: u32,
    players: Vec<(PlayerId, VisibilityGrid)>,
}

impl FogState {
    /// Build fresh (fully `Masked`) grids for each player. Fails loud on a
    /// degenerate map so no query ever runs against an empty grid.
    pub fn new(width: u32, height: u32, player_ids: &[PlayerId]) -> Result<Self, EngineError> {
        if width == 0 || height == 0 {
            return Err(EngineError::SimInvalidConfig {
                field: "fog.dimensions".to_string(),
                reason: "grid width and height must be non-zero".to_string(),
            });
        }
        if (width as u64) * (height as u64) > u32::MAX as u64 {
            return Err(EngineError::SimInvalidConfig {
                field: "fog.dimensions".to_string(),
                reason: "grid cell count exceeds u32".to_string(),
            });
        }
        let mut ids: Vec<PlayerId> = player_ids.to_vec();
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            return Err(EngineError::SimInvalidConfig {
                field: "fog.players".to_string(),
                reason: "at least one player is required".to_string(),
            });
        }
        let players = ids
            .into_iter()
            .map(|id| (id, VisibilityGrid::new(width, height)))
            .collect();
        Ok(Self {
            width,
            height,
            players,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Immutable grid for a player, if tracked.
    pub fn grid(&self, player: PlayerId) -> Option<&VisibilityGrid> {
        self.players
            .iter()
            .find(|(id, _)| *id == player)
            .map(|(_, grid)| grid)
    }

    /// Whether `player` currently sees cell `(x, y)` -- the sim targeting query.
    pub fn is_visible(&self, player: PlayerId, x: i32, y: i32) -> bool {
        self.grid(player)
            .map(|grid| grid.state(x, y) == VisibilityState::Visible)
            .unwrap_or(false)
    }

    /// State-of-the-frame counts for a player as `(masked, explored, visible)`.
    pub fn counts(&self, player: PlayerId) -> Option<(usize, usize, usize)> {
        self.grid(player).map(VisibilityGrid::counts)
    }

    /// Recompute every player's grid from the current vision sources and typed
    /// modifiers. Deterministic: demote last frame's `Visible` to `Explored`,
    /// union every shared sight disc (integer squared-distance test), then apply
    /// reveals, then conceals (conceal wins). Idempotent under identical input.
    pub fn recompute(
        &mut self,
        sources: &[VisionSource],
        modifiers: &[VisibilityModifier],
        alliance: &impl SharedVision,
    ) {
        for (player, grid) in &mut self.players {
            let player = *player;
            for cell in &mut grid.cells {
                if *cell == VisibilityState::Visible {
                    *cell = VisibilityState::Explored;
                }
            }
            for source in sources {
                if !alliance.shares_vision(player, source.owner) {
                    continue;
                }
                mark_disc(grid, source);
            }
            for modifier in modifiers.iter().filter(|m| m.kind == ModifierKind::Reveal) {
                if modifier.player == player {
                    fill_rect(grid, modifier, VisibilityState::Visible);
                }
            }
            for modifier in modifiers.iter().filter(|m| m.kind == ModifierKind::Conceal) {
                if modifier.player == player {
                    fill_rect(grid, modifier, VisibilityState::Masked);
                }
            }
        }
    }

    /// Order-independent FNV-1a64 digest of all grids -- the replay-determinism
    /// check hashes this after identical order streams and asserts equality.
    pub fn state_hash(&self) -> u64 {
        let mut hasher = Fnv1a64::new();
        hasher.update_u64_le(u64::from(self.width));
        hasher.update_u64_le(u64::from(self.height));
        hasher.update_u64_le(self.players.len() as u64);
        for (id, grid) in &self.players {
            hasher.update_u64_le(u64::from(*id));
            for cell in &grid.cells {
                hasher.update_u64_le(*cell as u64);
            }
        }
        hasher.finish()
    }
}

/// Map a fixed-point world coordinate to an integer cell using integer division
/// (deterministic, floor toward negative infinity). `cell_size_raw` is a
/// fixed-point cell edge length in raw units; must be > 0.
pub fn world_to_cell(
    coord_raw: i64,
    origin_raw: i64,
    cell_size_raw: i64,
) -> Result<i32, EngineError> {
    if cell_size_raw <= 0 {
        return Err(EngineError::SimInvalidConfig {
            field: "fog.cell_size".to_string(),
            reason: "cell size must be positive".to_string(),
        });
    }
    let offset = coord_raw - origin_raw;
    // Floor division so negative offsets map to the correct lower cell.
    let cell = offset.div_euclid(cell_size_raw);
    if cell < i32::MIN as i64 || cell > i32::MAX as i64 {
        return Err(EngineError::SimInvalidConfig {
            field: "fog.cell_index".to_string(),
            reason: "computed cell index exceeds i32".to_string(),
        });
    }
    Ok(cell as i32)
}

fn mark_disc(grid: &mut VisibilityGrid, source: &VisionSource) {
    let radius = source.sight_radius_cells as i64;
    let radius_sq = radius * radius;
    for y in 0..grid.height {
        for x in 0..grid.width {
            let dx = x as i64 - source.cell_x as i64;
            let dy = y as i64 - source.cell_y as i64;
            if dx * dx + dy * dy <= radius_sq {
                let index = grid.index(x, y);
                grid.cells[index] = VisibilityState::Visible;
            }
        }
    }
}

fn fill_rect(grid: &mut VisibilityGrid, modifier: &VisibilityModifier, state: VisibilityState) {
    let min_x = modifier.min_x.max(0);
    let min_y = modifier.min_y.max(0);
    let max_x = modifier.max_x.min(grid.width as i32 - 1);
    let max_y = modifier.max_y.min(grid.height as i32 - 1);
    if min_x > max_x || min_y > max_y {
        return;
    }
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let index = grid.index(x as u32, y as u32);
            grid.cells[index] = state;
        }
    }
}
