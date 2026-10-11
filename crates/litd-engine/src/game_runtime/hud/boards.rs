//! #840 leaderboards + multiboards: single-column leaderboards and multi-column
//! multiboards (scoreboards).
//!
//! Both are deterministic integer state updated from trigger/order calls (e.g. a
//! resource change bumps a leaderboard value). Values are `i64`; per-player
//! visibility is a [`PlayerMask`]. Sorting is a stable descending-by-value sort with
//! a label tiebreak so identical inputs always produce an identical row order.

use litd_hash::Fnv1a64;

use crate::EngineError;

use super::{HudColor, PlayerId, PlayerMask, hash_str, invalid};

/// One leaderboard row: a label and an integer value (WC3 `LeaderboardSetItemValue`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaderboardRow {
    pub label: String,
    pub value: i64,
}

/// A single-column leaderboard (WC3 leaderboards show one ranked value per player).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaderboard {
    pub title: String,
    pub rows: Vec<LeaderboardRow>,
    pub visible_to: PlayerMask,
    pub shown: bool,
}

impl Leaderboard {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            rows: Vec::new(),
            visible_to: PlayerMask::all(),
            shown: true,
        }
    }

    pub fn set_row(&mut self, label: &str, value: i64) {
        if let Some(row) = self.rows.iter_mut().find(|r| r.label == label) {
            row.value = value;
        } else {
            self.rows.push(LeaderboardRow {
                label: label.to_string(),
                value,
            });
        }
    }

    /// Stable descending sort by value, label as tiebreak (deterministic ranking).
    pub fn sort_descending(&mut self) {
        self.rows
            .sort_by(|a, b| b.value.cmp(&a.value).then_with(|| a.label.cmp(&b.label)));
    }

    fn hash(&self, hasher: &mut Fnv1a64) {
        hash_str(hasher, &self.title);
        hasher.update_u64_le(u64::from(self.shown));
        hasher.update_u64_le(self.visible_to.bits());
        hasher.update_u64_le(self.rows.len() as u64);
        for row in &self.rows {
            hash_str(hasher, &row.label);
            hasher.update_i64_le(row.value);
        }
    }
}

/// One multiboard cell: text + optional icon + integer value with per-field show flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiboardCell {
    pub text: String,
    pub icon: String,
    pub value: i64,
    pub show_text: bool,
    pub show_icon: bool,
    pub color: HudColor,
}

impl Default for MultiboardCell {
    fn default() -> Self {
        Self {
            text: String::new(),
            icon: String::new(),
            value: 0,
            show_text: true,
            show_icon: false,
            color: HudColor::WHITE,
        }
    }
}

impl MultiboardCell {
    fn hash(&self, hasher: &mut Fnv1a64) {
        hash_str(hasher, &self.text);
        hash_str(hasher, &self.icon);
        hasher.update_i64_le(self.value);
        hasher.update_u64_le(u64::from(self.show_text));
        hasher.update_u64_le(u64::from(self.show_icon));
        self.color.hash(hasher);
    }
}

/// A multi-column multiboard (WC3 multiboards: a rows x cols grid of cells).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Multiboard {
    pub title: String,
    pub rows: usize,
    pub cols: usize,
    cells: Vec<MultiboardCell>,
    pub visible_to: PlayerMask,
    pub shown: bool,
}

impl Multiboard {
    /// Create a `rows` x `cols` multiboard. Fails loud on a zero dimension.
    pub fn new(title: &str, rows: usize, cols: usize) -> Result<Self, EngineError> {
        if rows == 0 || cols == 0 {
            return Err(invalid(
                "multiboard.dimensions",
                format!("rows ({rows}) and cols ({cols}) must both be positive"),
            ));
        }
        Ok(Self {
            title: title.to_string(),
            rows,
            cols,
            cells: vec![MultiboardCell::default(); rows * cols],
            visible_to: PlayerMask::all(),
            shown: true,
        })
    }

    fn index(&self, row: usize, col: usize) -> Result<usize, EngineError> {
        if row >= self.rows || col >= self.cols {
            return Err(invalid(
                "multiboard.cell",
                format!(
                    "cell ({row},{col}) out of bounds for {}x{}",
                    self.rows, self.cols
                ),
            ));
        }
        Ok(row * self.cols + col)
    }

    pub fn cell(&self, row: usize, col: usize) -> Result<&MultiboardCell, EngineError> {
        let index = self.index(row, col)?;
        Ok(&self.cells[index])
    }

    pub fn cell_mut(&mut self, row: usize, col: usize) -> Result<&mut MultiboardCell, EngineError> {
        let index = self.index(row, col)?;
        Ok(&mut self.cells[index])
    }

    /// Set a cell's text + value in one call (the common scoreboard update).
    pub fn set_cell(
        &mut self,
        row: usize,
        col: usize,
        text: &str,
        value: i64,
    ) -> Result<(), EngineError> {
        let cell = self.cell_mut(row, col)?;
        cell.text = text.to_string();
        cell.value = value;
        Ok(())
    }

    fn hash(&self, hasher: &mut Fnv1a64) {
        hash_str(hasher, &self.title);
        hasher.update_u64_le(self.rows as u64);
        hasher.update_u64_le(self.cols as u64);
        hasher.update_u64_le(u64::from(self.shown));
        hasher.update_u64_le(self.visible_to.bits());
        for cell in &self.cells {
            cell.hash(hasher);
        }
    }
}

/// A stable identifier for a multiboard within a [`BoardSet`].
pub type MultiboardId = u32;

/// The full board state: at most one leaderboard (WC3 shows one at a time) plus a
/// set of multiboards keyed by insertion id (deterministic vec order).
#[derive(Debug, Default)]
pub struct BoardSet {
    leaderboard: Option<Leaderboard>,
    multiboards: Vec<(MultiboardId, Multiboard)>,
}

impl BoardSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_leaderboard(&mut self, leaderboard: Leaderboard) {
        self.leaderboard = Some(leaderboard);
    }

    pub fn leaderboard(&self) -> Option<&Leaderboard> {
        self.leaderboard.as_ref()
    }

    pub fn leaderboard_mut(&mut self) -> Option<&mut Leaderboard> {
        self.leaderboard.as_mut()
    }

    /// Insert a multiboard under `id`; fails loud on a duplicate id.
    pub fn insert_multiboard(
        &mut self,
        id: MultiboardId,
        board: Multiboard,
    ) -> Result<(), EngineError> {
        if self.multiboards.iter().any(|(existing, _)| *existing == id) {
            return Err(invalid(
                "multiboard.id",
                format!("multiboard id {id} already exists"),
            ));
        }
        self.multiboards.push((id, board));
        Ok(())
    }

    pub fn multiboard(&self, id: MultiboardId) -> Option<&Multiboard> {
        self.multiboards
            .iter()
            .find(|(existing, _)| *existing == id)
            .map(|(_, board)| board)
    }

    pub fn multiboard_mut(&mut self, id: MultiboardId) -> Option<&mut Multiboard> {
        self.multiboards
            .iter_mut()
            .find(|(existing, _)| *existing == id)
            .map(|(_, board)| board)
    }

    /// Set a multiboard's per-player visibility (WC3 `MultiboardDisplay` is global,
    /// but the runtime host filters per player from this mask).
    pub fn set_multiboard_visibility(
        &mut self,
        id: MultiboardId,
        visible_to: PlayerMask,
    ) -> Result<(), EngineError> {
        let board = self
            .multiboard_mut(id)
            .ok_or_else(|| invalid("multiboard.id", format!("multiboard id {id} not found")))?;
        board.visible_to = visible_to;
        Ok(())
    }

    /// Whether a multiboard is visible to `player` (shown AND in the mask).
    pub fn is_multiboard_visible(&self, id: MultiboardId, player: PlayerId) -> bool {
        self.multiboard(id)
            .is_some_and(|board| board.shown && board.visible_to.contains(player))
    }

    pub(crate) fn hash(&self, hasher: &mut Fnv1a64) {
        hasher.update(b"boards");
        hasher.update_u64_le(u64::from(self.leaderboard.is_some()));
        if let Some(board) = &self.leaderboard {
            board.hash(hasher);
        }
        hasher.update_u64_le(self.multiboards.len() as u64);
        for (id, board) in &self.multiboards {
            hasher.update_u64_le(u64::from(*id));
            board.hash(hasher);
        }
    }
}
