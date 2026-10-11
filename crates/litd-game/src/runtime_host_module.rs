use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use litd_engine::{
    EngineError,
    app::{AppModule, FrameTick},
    game_package::RuntimePackageReadback,
    game_runtime::{CampaignFlow, MatchRuntimeSession, OrderTargetKind, SessionOrder},
};

use crate::runtime_state::RuntimeCounterState;

#[path = "runtime_host_text.rs"]
mod runtime_host_text;
pub(crate) use runtime_host_text::RuntimeTextSnapshot;

/// #1852: the save slot a campaign package's flow runner boots into by default.
const CAMPAIGN_DEFAULT_SAVE_SLOT: u32 = 1;

#[derive(Default)]
pub(super) struct RuntimeCounters {
    starts: AtomicU64,
    fixed_ticks: AtomicU64,
    updates: AtomicU64,
    shutdowns: AtomicU64,
    // #1843: composed match-runtime session evidence, surfaced into the runtime state.
    session_active: AtomicBool,
    session_ticks: AtomicU64,
    session_state_hash: AtomicU64,
    /// #1894: bits 0..=3 set when host observes QWER ability just_pressed.
    ability_slot_mask: AtomicU64,
    ability_cast_accepted: AtomicU64,
    ability_cast_rejected: AtomicU64,
    last_ability_slot: AtomicU64,
    last_ability_id: AtomicU64,
    runtime_text: RwLock<RuntimeTextSnapshot>,
}

impl RuntimeCounters {
    /// Host: mark ability slots 0..=3 pressed this frame (#1894).
    pub(super) fn queue_ability_slots(&self, mask: u8) {
        if mask != 0 {
            self.ability_slot_mask
                .fetch_or(u64::from(mask), Ordering::Relaxed);
        }
    }

    pub(super) fn snapshot(&self) -> RuntimeCounterState {
        let session_active = self.session_active.load(Ordering::Relaxed);
        RuntimeCounterState {
            runtime_start_calls: self.starts.load(Ordering::Relaxed),
            fixed_tick_total: self.fixed_ticks.load(Ordering::Relaxed),
            update_total: self.updates.load(Ordering::Relaxed),
            runtime_shutdown_calls: self.shutdowns.load(Ordering::Relaxed),
            session_active,
            session_ticks: self.session_ticks.load(Ordering::Relaxed),
            session_state_hash: if session_active {
                format!("{:016x}", self.session_state_hash.load(Ordering::Relaxed))
            } else {
                String::new()
            },
            ability_cast_accepted: self.ability_cast_accepted.load(Ordering::Relaxed),
            ability_cast_rejected: self.ability_cast_rejected.load(Ordering::Relaxed),
            last_ability_slot: self.last_ability_slot.load(Ordering::Relaxed) as u8,
            last_ability_id: self.last_ability_id.load(Ordering::Relaxed) as u16,
        }
    }
}

/// The sim-side runtime module. When the loaded package carries lowered runtime datasets
/// (#1843), it instantiates the composed [`MatchRuntimeSession`] from that PACKAGE data
/// and advances it one deterministic tick per fixed update, so `litd-game` actually runs
/// the Wave-4 systems (economy/match/fog/day-night/HUD) rather than an empty counter.
pub(super) struct GameRuntimeModule {
    counters: Arc<RuntimeCounters>,
    session: Option<MatchRuntimeSession>,
}

impl GameRuntimeModule {
    /// Build the module and, if the package carries runtime datasets, instantiate the
    /// composed session from them. A corrupt dataset already failed the package load, so
    /// reaching here with a present-but-invalid manifest is impossible; a build failure
    /// (e.g. degenerate fog grid) is surfaced loud.
    pub(super) fn from_package(
        counters: Arc<RuntimeCounters>,
        package: &RuntimePackageReadback,
    ) -> Result<Self, EngineError> {
        let session = Self::boot_session(package)?;
        counters.publish_runtime_text(session.as_ref())?;
        if let Some(session) = &session {
            counters.session_active.store(true, Ordering::Relaxed);
            counters
                .session_state_hash
                .store(session.combined_state_hash(), Ordering::Relaxed);
        }
        Ok(Self { counters, session })
    }

    /// Resolve which match session (if any) this package boots into:
    /// - a #1843 single-map runtime package boots its embedded datasets directly;
    /// - a #1852 campaign package boots its ENTRY chapter's match via the campaign flow
    ///   runner (a cinematic entry chapter that embeds no datasets boots no session);
    /// - a plain package (no runtime, no campaign) boots no session (tick counter only).
    fn boot_session(
        package: &RuntimePackageReadback,
    ) -> Result<Option<MatchRuntimeSession>, EngineError> {
        if let Some(manifest) = &package.runtime {
            let session = MatchRuntimeSession::from_manifest(manifest)
                .map_err(|error| runtime_host_error("session.build", error.to_string()))?;
            return Ok(Some(session));
        }
        if let Some(campaign) = &package.campaign {
            let flow = CampaignFlow::boot(campaign.clone(), CAMPAIGN_DEFAULT_SAVE_SLOT)
                .map_err(|error| runtime_host_error("campaign.boot", error.to_string()))?;
            if flow.current_chapter_boots_match() {
                let session = flow
                    .instantiate_current_session()
                    .map_err(|error| runtime_host_error("campaign.session", error.to_string()))?;
                return Ok(Some(session));
            }
        }
        Ok(None)
    }

    /// #1894: map ability bar slots to the Nth capability on the local selection
    /// (MOBA-style QWER). Rejects loud with counters when selection/capability missing.
    fn drain_ability_slots(&mut self) -> Result<(), EngineError> {
        let mask = self.counters.ability_slot_mask.swap(0, Ordering::Relaxed) as u8;
        if mask == 0 {
            return Ok(());
        }
        let Some(session) = &mut self.session else {
            for slot in 0u8..4 {
                if mask & (1 << slot) != 0 {
                    self.counters
                        .ability_cast_rejected
                        .fetch_add(1, Ordering::Relaxed);
                    self.counters
                        .last_ability_slot
                        .store(u64::from(slot), Ordering::Relaxed);
                    tracing::warn!(
                        code = "LITD-ABILITY-SLOT-NO-SESSION",
                        slot,
                        "ability slot pressed but package has no match session"
                    );
                }
            }
            return Ok(());
        };

        const LOCAL_PLAYER: u8 = 0;
        for slot in 0u8..4 {
            if mask & (1 << slot) == 0 {
                continue;
            }
            self.counters
                .last_ability_slot
                .store(u64::from(slot), Ordering::Relaxed);
            match cast_ability_slot(session, LOCAL_PLAYER, slot) {
                Ok(ability_id) => {
                    self.counters
                        .ability_cast_accepted
                        .fetch_add(1, Ordering::Relaxed);
                    self.counters
                        .last_ability_id
                        .store(u64::from(ability_id), Ordering::Relaxed);
                    self.counters
                        .session_state_hash
                        .store(session.combined_state_hash(), Ordering::Relaxed);
                    tracing::info!(
                        code = "LITD-ABILITY-SLOT-CAST",
                        slot,
                        ability_id,
                        "MOBA ability slot cast accepted"
                    );
                }
                Err(error) => {
                    self.counters
                        .ability_cast_rejected
                        .fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        code = "LITD-ABILITY-SLOT-REJECTED",
                        slot,
                        error = %error,
                        "MOBA ability slot cast rejected"
                    );
                }
            }
        }
        Ok(())
    }
}

/// Resolve selection (or first owned unit), map slot -> Nth capability, issue order.
fn cast_ability_slot(
    session: &mut MatchRuntimeSession,
    player: u8,
    slot: u8,
) -> Result<u16, EngineError> {
    let match_state = session.match_state();
    let mut units: Vec<u32> = match_state
        .selection()
        .selection(usize::from(player))
        .map_err(|e| runtime_host_error("ability.selection", e.to_string()))?
        .members()
        .to_vec();
    if units.is_empty() {
        units = match_state.alive_owned_units(player);
        if units.is_empty() {
            return Err(runtime_host_error(
                "ability.selection",
                format!("player {player} has no selected or owned units for ability slot {slot}"),
            ));
        }
        // Auto-select first owned unit so subsequent orders have a selection.
        let first = units[0];
        session
            .apply(SessionOrder::SelectUnits {
                player,
                units: vec![first],
            })
            .map_err(|e| runtime_host_error("ability.select", e.to_string()))?;
        units = vec![first];
    }

    let primary = units[0];
    let caps = session
        .match_state()
        .capabilities(primary)
        .map_err(|e| runtime_host_error("ability.caps", e.to_string()))?;
    let cap = caps.get(usize::from(slot)).ok_or_else(|| {
        runtime_host_error(
            "ability.slot",
            format!(
                "unit {primary} has {} capabilities; slot {slot} is empty (QWER bar)",
                caps.len()
            ),
        )
    })?;
    let ability = cap.ability;
    let order = match cap.target_kind {
        OrderTargetKind::None => SessionOrder::IssueAbilityNoTarget {
            player,
            units: units.clone(),
            ability,
            queued: false,
        },
        OrderTargetKind::Point => {
            // Ground under the unit (or field cell if available).
            let (x, z) = session
                .field_unit(primary)
                .map(|u| (u.cell_x, u.cell_z))
                .unwrap_or((0, 0));
            SessionOrder::IssueMove {
                player,
                units: units.clone(),
                ability,
                x,
                z,
                queued: false,
            }
        }
        OrderTargetKind::Unit => {
            return Err(runtime_host_error(
                "ability.target",
                format!(
                    "ability {ability} on slot {slot} requires a unit target; click-target UI not yet wired"
                ),
            ));
        }
    };
    session
        .apply(order)
        .map_err(|e| runtime_host_error("ability.cast", e.to_string()))?;
    Ok(ability)
}

impl AppModule for GameRuntimeModule {
    fn name(&self) -> &'static str {
        "game.runtime"
    }

    fn on_start(&mut self) -> Result<(), EngineError> {
        self.counters.starts.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn on_fixed_update(&mut self, _tick: &FrameTick) -> Result<(), EngineError> {
        self.counters.fixed_ticks.fetch_add(1, Ordering::Relaxed);
        if let Some(session) = &mut self.session {
            // Advance the composed runtime one deterministic tick from package data.
            let report = session
                .apply(SessionOrder::Tick)
                .map_err(|error| runtime_host_error("session.tick", error.to_string()))?;
            self.counters
                .session_ticks
                .store(report.applied_tick, Ordering::Relaxed);
            self.counters
                .session_state_hash
                .store(report.combined_state_hash, Ordering::Relaxed);
            self.counters.publish_runtime_text(Some(session))?;
        }
        Ok(())
    }

    fn on_update(&mut self, _tick: &FrameTick) -> Result<(), EngineError> {
        self.counters.updates.fetch_add(1, Ordering::Relaxed);
        // #1894: drain QWER ability-slot presses into session cast orders.
        self.drain_ability_slots()?;
        Ok(())
    }

    fn on_shutdown(&mut self) -> Result<(), EngineError> {
        self.counters.shutdowns.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

pub(super) fn runtime_host_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    let field = format!("litd_game.runtime.{}", field.into());
    let reason = reason.into();
    tracing::error!(
        field = %field,
        reason = %reason,
        code = "LITD-GAME-RUNTIME-HOST",
        "litd-game runtime host failed"
    );
    EngineError::GamePackageInvalidConfig { field, reason }
}
