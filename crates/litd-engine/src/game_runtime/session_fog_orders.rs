//! #1857: session dispatch for the fog reveal/conceal orders.
//!
//! `RevealRegion`/`ConcealRegion` mutate the session-held modifier list, kept sorted
//! and unique so it is canonical regardless of insertion order, then recompute the fog
//! immediately so the change is visible this order and reflected in the fog grid hash.
//! The modifier list itself is also folded into `combined_state_hash`, so a reveal is
//! deterministic and lockstep-safe (#85): identical order streams reproduce identical
//! fog. Fails loud (typed) on an out-of-range player or an inverted rectangle -- the
//! anagnorisis beat "the fog lifting on the truth" (M4) must never silently no-op.

use super::super::fog::{ModifierKind, VisibilityModifier};
use super::GameRuntimeError;
use super::{MatchRuntimeSession, SessionOrder};

impl MatchRuntimeSession {
    /// Apply a `RevealRegion`/`ConcealRegion` order: validate, dedupe into the
    /// session modifier list, then recompute fog.
    pub(super) fn apply_visibility_modifier(
        &mut self,
        order: &SessionOrder,
    ) -> Result<(), GameRuntimeError> {
        let (player, kind, min_x, min_y, max_x, max_y) = match order {
            SessionOrder::RevealRegion {
                player,
                min_x,
                min_y,
                max_x,
                max_y,
            } => (
                *player,
                ModifierKind::Reveal,
                *min_x,
                *min_y,
                *max_x,
                *max_y,
            ),
            SessionOrder::ConcealRegion {
                player,
                min_x,
                min_y,
                max_x,
                max_y,
            } => (
                *player,
                ModifierKind::Conceal,
                *min_x,
                *min_y,
                *max_x,
                *max_y,
            ),
            _ => {
                return Err(GameRuntimeError::Invalid {
                    field: "session.fog_modifier".to_string(),
                    reason: "dispatched a non-fog-modifier order".to_string(),
                });
            }
        };
        let player_count = self.match_state.roster().player_count() as u32;
        if player >= player_count {
            return Err(GameRuntimeError::Invalid {
                field: "session.fog_modifier.player".to_string(),
                reason: format!("player {player} is out of range for {player_count} players"),
            });
        }
        if min_x > max_x || min_y > max_y {
            return Err(GameRuntimeError::Invalid {
                field: "session.fog_modifier.rect".to_string(),
                reason: format!("inverted rectangle ({min_x},{min_y})..({max_x},{max_y})"),
            });
        }
        let modifier = VisibilityModifier {
            player,
            kind,
            min_x,
            min_y,
            max_x,
            max_y,
        };
        // Insert sorted + unique so the list is canonical (deterministic) regardless
        // of the order reveals/conceals were authored in.
        if let Err(pos) = self
            .fog_modifiers
            .binary_search_by(|m| modifier_key(m).cmp(&modifier_key(&modifier)))
        {
            self.fog_modifiers.insert(pos, modifier);
        }
        self.recompute_fog()
    }

    /// The live fog reveal/conceal modifier list (read surface for host/FSV).
    pub fn fog_modifiers(&self) -> &[VisibilityModifier] {
        &self.fog_modifiers
    }
}

/// Total-order key for a modifier so the session list sorts canonically.
fn modifier_key(m: &VisibilityModifier) -> (u32, u8, i32, i32, i32, i32) {
    let kind = match m.kind {
        ModifierKind::Reveal => 0,
        ModifierKind::Conceal => 1,
    };
    (m.player, kind, m.min_x, m.min_y, m.max_x, m.max_y)
}
