//! #842 dialogs + buttons: modal dialog state with per-player display and
//! button-click resolution as a typed sim order.
//!
//! A dialog click enters the world as a [`super::HudOrderKind::DialogClick`] on the
//! order stream and is resolved by [`DialogSet::resolve_click`]; because the click's
//! tick is part of the deterministic stream, a replay with the same click at the same
//! tick reproduces the identical dialog outcome and state hash (#85 lockstep). The
//! click record is retained so the outcome contributes to [`GameHudWorld::state_hash`].

use litd_hash::Fnv1a64;

use crate::EngineError;

use super::{PlayerId, PlayerMask, hash_str, invalid};

/// Stable dialog / button identifiers within a [`DialogSet`].
pub type DialogId = u32;
pub type ButtonId = u32;

/// One dialog button (WC3 `DialogAddButton`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogButton {
    pub id: ButtonId,
    pub text: String,
    /// Optional hotkey as a key code (0 = none); integer so it hashes deterministically.
    pub hotkey: u32,
}

impl DialogButton {
    fn hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(u64::from(self.id));
        hash_str(hasher, &self.text);
        hasher.update_u64_le(u64::from(self.hotkey));
    }
}

/// A modal dialog: a title, an ordered button set, and per-player display state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    pub id: DialogId,
    pub title: String,
    pub buttons: Vec<DialogButton>,
    /// The players the dialog is currently displayed to (WC3 `DialogDisplay` is per
    /// player). Empty when hidden for everyone.
    pub shown_to: PlayerMask,
    next_button_id: ButtonId,
}

impl Dialog {
    fn hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(u64::from(self.id));
        hash_str(hasher, &self.title);
        hasher.update_u64_le(self.shown_to.bits());
        hasher.update_u64_le(self.buttons.len() as u64);
        for button in &self.buttons {
            button.hash(hasher);
        }
    }
}

/// A resolved dialog click retained for determinism + evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialogClickRecord {
    pub tick: u64,
    pub dialog: DialogId,
    pub button: ButtonId,
    pub player: PlayerId,
}

/// The dialog subsystem: all dialogs + the ordered click history.
#[derive(Debug, Default)]
pub struct DialogSet {
    dialogs: Vec<Dialog>,
    clicks: Vec<DialogClickRecord>,
    next_id: DialogId,
}

impl DialogSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn dialogs(&self) -> &[Dialog] {
        &self.dialogs
    }

    pub fn clicks(&self) -> &[DialogClickRecord] {
        &self.clicks
    }

    pub fn last_click(&self) -> Option<DialogClickRecord> {
        self.clicks.last().copied()
    }

    pub fn dialog(&self, id: DialogId) -> Option<&Dialog> {
        self.dialogs.iter().find(|d| d.id == id)
    }

    /// Create a dialog with a title. Fails loud on an empty title.
    pub fn create_dialog(&mut self, title: &str) -> Result<DialogId, EngineError> {
        if title.trim().is_empty() {
            return Err(invalid("dialog.title", "dialog title must be non-empty"));
        }
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| invalid("dialog.id", "dialog id counter overflowed"))?;
        self.dialogs.push(Dialog {
            id,
            title: title.to_string(),
            buttons: Vec::new(),
            shown_to: PlayerMask::EMPTY,
            next_button_id: 0,
        });
        Ok(id)
    }

    /// Add a button to a dialog, returning its id. Fails loud on empty text.
    pub fn add_button(
        &mut self,
        dialog: DialogId,
        text: &str,
        hotkey: u32,
    ) -> Result<ButtonId, EngineError> {
        if text.trim().is_empty() {
            return Err(invalid(
                "dialog.button.text",
                "button text must be non-empty",
            ));
        }
        let dialog = self.dialog_mut(dialog)?;
        let button_id = dialog.next_button_id;
        dialog.next_button_id = dialog
            .next_button_id
            .checked_add(1)
            .ok_or_else(|| invalid("dialog.button.id", "button id counter overflowed"))?;
        dialog.buttons.push(DialogButton {
            id: button_id,
            text: text.to_string(),
            hotkey,
        });
        Ok(button_id)
    }

    /// Show a dialog to a set of players (WC3 `DialogDisplay(..., true)`).
    pub fn show(&mut self, dialog: DialogId, players: PlayerMask) -> Result<(), EngineError> {
        self.dialog_mut(dialog)?.shown_to = players;
        Ok(())
    }

    /// Hide a dialog from everyone.
    pub fn hide(&mut self, dialog: DialogId) -> Result<(), EngineError> {
        self.dialog_mut(dialog)?.shown_to = PlayerMask::EMPTY;
        Ok(())
    }

    pub fn is_shown_to(&self, dialog: DialogId, player: PlayerId) -> bool {
        self.dialog(dialog)
            .is_some_and(|d| d.shown_to.contains(player))
    }

    /// Resolve a button click into the deterministic state: validate the dialog is
    /// shown to the clicking player and the button exists, record the click, and hide
    /// the dialog for that player (a modal dialog dismisses on click). This is the ONLY
    /// mutation driven by the order stream, so replays reproduce it exactly.
    pub fn resolve_click(
        &mut self,
        dialog: DialogId,
        button: ButtonId,
        player: PlayerId,
        tick: u64,
    ) -> Result<DialogClickRecord, EngineError> {
        let record = {
            let dialog_ref = self
                .dialog(dialog)
                .ok_or_else(|| invalid("dialog.id", format!("dialog {dialog} not found")))?;
            if !dialog_ref.shown_to.contains(player) {
                return Err(invalid(
                    "dialog.click.visibility",
                    format!("dialog {dialog} is not shown to player {player}"),
                ));
            }
            if !dialog_ref.buttons.iter().any(|b| b.id == button) {
                return Err(invalid(
                    "dialog.click.button",
                    format!("button {button} does not exist on dialog {dialog}"),
                ));
            }
            DialogClickRecord {
                tick,
                dialog,
                button,
                player,
            }
        };
        // Dismiss for the clicking player only (deterministic mask update).
        let dialog_mut = self.dialog_mut(dialog)?;
        let remaining = dialog_mut.shown_to.bits() & !(1_u64 << player);
        dialog_mut.shown_to = PlayerMask::from_bits(remaining);
        self.clicks.push(record);
        Ok(record)
    }

    fn dialog_mut(&mut self, id: DialogId) -> Result<&mut Dialog, EngineError> {
        self.dialogs
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| invalid("dialog.id", format!("dialog {id} not found")))
    }

    pub(crate) fn hash(&self, hasher: &mut Fnv1a64) {
        hasher.update(b"dialogs");
        hasher.update_u64_le(self.dialogs.len() as u64);
        for dialog in &self.dialogs {
            dialog.hash(hasher);
        }
        hasher.update_u64_le(self.clicks.len() as u64);
        for click in &self.clicks {
            hasher.update_u64_le(click.tick);
            hasher.update_u64_le(u64::from(click.dialog));
            hasher.update_u64_le(u64::from(click.button));
            hasher.update_u64_le(u64::from(click.player));
        }
    }
}
