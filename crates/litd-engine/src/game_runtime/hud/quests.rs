//! #839 quests + quest log: deterministic quest lifecycle (create/update/complete/
//! fail), quest-log state, and discovery notifications.
//!
//! Quest DEFINITIONS (title/description/icon/required flags) are authored content
//! lowered through Calyx (see [`super::quests_authoring`]); the RUNTIME state here
//! is advanced only by trigger calls, never the wall clock. Every mutation records
//! a [`QuestNotification`] stamped with the sim tick so the log panel can surface a
//! deterministic discovery/update/complete/fail feed.

use litd_hash::Fnv1a64;

use crate::EngineError;

use super::{hash_str, invalid};

/// A stable quest identifier (authoring id, hashed as bytes for determinism).
pub type QuestId = u32;

/// A required-flag identifier. Flags are integer ids so satisfied-sets sort and
/// hash deterministically (no string map order).
pub type FlagId = u32;

/// Lifecycle state of a quest. `Active` quests may complete or fail exactly once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestProgress {
    Active,
    Completed,
    Failed,
}

impl QuestProgress {
    fn tag(self) -> u64 {
        match self {
            Self::Active => 1,
            Self::Completed => 2,
            Self::Failed => 3,
        }
    }
}

/// Kind of quest-log notification, mirroring WC3's quest discovery/update popups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestNotificationKind {
    Discovered,
    Updated,
    Completed,
    Failed,
}

impl QuestNotificationKind {
    fn tag(self) -> u64 {
        match self {
            Self::Discovered => 1,
            Self::Updated => 2,
            Self::Completed => 3,
            Self::Failed => 4,
        }
    }
}

/// A single deterministic quest-log notification (the discovery feed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestNotification {
    pub tick: u64,
    pub quest: QuestId,
    pub kind: QuestNotificationKind,
}

/// A quest definition + its live runtime state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quest {
    pub id: QuestId,
    pub title: String,
    pub description: String,
    pub icon: String,
    /// A required (main) quest vs an optional side quest -- affects log grouping only.
    pub required: bool,
    /// Flags that must all be satisfied before [`QuestLog::try_complete`] succeeds.
    pub required_flags: Vec<FlagId>,
    pub progress: QuestProgress,
    pub discovered: bool,
    pub completed_tick: Option<u64>,
}

impl Quest {
    fn hash(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(u64::from(self.id));
        hash_str(hasher, &self.title);
        hash_str(hasher, &self.description);
        hash_str(hasher, &self.icon);
        hasher.update_u64_le(u64::from(self.required));
        hasher.update_u64_le(self.required_flags.len() as u64);
        for flag in &self.required_flags {
            hasher.update_u64_le(u64::from(*flag));
        }
        hasher.update_u64_le(self.progress.tag());
        hasher.update_u64_le(u64::from(self.discovered));
        hasher.update_u64_le(self.completed_tick.map_or(0, |t| t.wrapping_add(1)));
    }
}

/// The quest log: the ordered set of quests + satisfied flags + notification feed.
#[derive(Debug, Default)]
pub struct QuestLog {
    quests: Vec<Quest>,
    satisfied_flags: Vec<FlagId>,
    notifications: Vec<QuestNotification>,
}

impl QuestLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn quests(&self) -> &[Quest] {
        &self.quests
    }

    pub fn notifications(&self) -> &[QuestNotification] {
        &self.notifications
    }

    pub fn quest(&self, id: QuestId) -> Option<&Quest> {
        self.quests.iter().find(|q| q.id == id)
    }

    pub fn is_flag_satisfied(&self, flag: FlagId) -> bool {
        self.satisfied_flags.binary_search(&flag).is_ok()
    }

    /// Create a quest. Fails loud on a duplicate id or an empty title/description --
    /// the negative control (sabotaged quest data) trips exactly here.
    #[allow(clippy::too_many_arguments)]
    pub fn create_quest(
        &mut self,
        id: QuestId,
        title: &str,
        description: &str,
        icon: &str,
        required: bool,
        required_flags: &[FlagId],
    ) -> Result<(), EngineError> {
        if title.trim().is_empty() {
            return Err(invalid("quest.title", "quest title must be non-empty"));
        }
        if description.trim().is_empty() {
            return Err(invalid(
                "quest.description",
                "quest description must be non-empty",
            ));
        }
        if self.quests.iter().any(|q| q.id == id) {
            return Err(invalid(
                "quest.id",
                format!("quest id {id} already exists in the log"),
            ));
        }
        let mut flags = required_flags.to_vec();
        flags.sort_unstable();
        flags.dedup();
        self.quests.push(Quest {
            id,
            title: title.to_string(),
            description: description.to_string(),
            icon: icon.to_string(),
            required,
            required_flags: flags,
            progress: QuestProgress::Active,
            discovered: false,
            completed_tick: None,
        });
        Ok(())
    }

    /// Mark a quest as discovered (WC3 `QuestSetDiscovered`), emitting a discovery
    /// notification the first time only.
    pub fn discover_quest(&mut self, id: QuestId, tick: u64) -> Result<(), EngineError> {
        let already = self.quest_mut(id)?.discovered;
        if already {
            return Ok(());
        }
        self.quest_mut(id)?.discovered = true;
        self.notify(id, QuestNotificationKind::Discovered, tick);
        Ok(())
    }

    /// Update a quest's description (WC3 `QuestSetDescription`), emitting an update.
    pub fn update_description(
        &mut self,
        id: QuestId,
        description: &str,
        tick: u64,
    ) -> Result<(), EngineError> {
        if description.trim().is_empty() {
            return Err(invalid(
                "quest.description",
                "updated quest description must be non-empty",
            ));
        }
        self.quest_mut(id)?.description = description.to_string();
        self.notify(id, QuestNotificationKind::Updated, tick);
        Ok(())
    }

    /// Record that a required flag is now satisfied (a trigger condition became true).
    pub fn satisfy_flag(&mut self, flag: FlagId) {
        if let Err(pos) = self.satisfied_flags.binary_search(&flag) {
            self.satisfied_flags.insert(pos, flag);
        }
    }

    /// Try to complete a quest: succeeds only if every required flag is satisfied.
    /// Fails loud otherwise -- a completion with unmet requirements is sabotaged data.
    pub fn try_complete(&mut self, id: QuestId, tick: u64) -> Result<(), EngineError> {
        let missing: Vec<FlagId> = {
            let quest = self.quest(id).ok_or_else(|| self.missing(id))?;
            if quest.progress != QuestProgress::Active {
                return Err(invalid(
                    "quest.progress",
                    "only an active quest can be completed",
                ));
            }
            quest
                .required_flags
                .iter()
                .copied()
                .filter(|f| !self.is_flag_satisfied(*f))
                .collect()
        };
        if !missing.is_empty() {
            return Err(invalid(
                "quest.required_flags",
                format!(
                    "cannot complete quest {id}: {} flag(s) unmet",
                    missing.len()
                ),
            ));
        }
        let quest = self.quest_mut(id)?;
        quest.progress = QuestProgress::Completed;
        quest.completed_tick = Some(tick);
        self.notify(id, QuestNotificationKind::Completed, tick);
        Ok(())
    }

    /// Fail an active quest (WC3 `QuestSetFailed`).
    pub fn fail_quest(&mut self, id: QuestId, tick: u64) -> Result<(), EngineError> {
        let quest = self.quest_mut(id)?;
        if quest.progress != QuestProgress::Active {
            return Err(invalid(
                "quest.progress",
                "only an active quest can be failed",
            ));
        }
        quest.progress = QuestProgress::Failed;
        self.notify(id, QuestNotificationKind::Failed, tick);
        Ok(())
    }

    fn quest_mut(&mut self, id: QuestId) -> Result<&mut Quest, EngineError> {
        self.quests
            .iter_mut()
            .find(|q| q.id == id)
            .ok_or_else(|| invalid("quest.id", format!("quest id {id} not found in the log")))
    }

    fn missing(&self, id: QuestId) -> EngineError {
        invalid("quest.id", format!("quest id {id} not found in the log"))
    }

    fn notify(&mut self, quest: QuestId, kind: QuestNotificationKind, tick: u64) {
        self.notifications
            .push(QuestNotification { tick, quest, kind });
    }

    pub(crate) fn hash(&self, hasher: &mut Fnv1a64) {
        hasher.update(b"quests");
        hasher.update_u64_le(self.quests.len() as u64);
        for quest in &self.quests {
            quest.hash(hasher);
        }
        hasher.update_u64_le(self.satisfied_flags.len() as u64);
        for flag in &self.satisfied_flags {
            hasher.update_u64_le(u64::from(*flag));
        }
        hasher.update_u64_le(self.notifications.len() as u64);
        for note in &self.notifications {
            hasher.update_u64_le(note.tick);
            hasher.update_u64_le(u64::from(note.quest));
            hasher.update_u64_le(note.kind.tag());
        }
    }
}
