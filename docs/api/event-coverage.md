# Event coverage

<!-- GENERATED from docs/api/event-coverage.json by jassgen -eventcov (#466).
     Do not edit by hand; TestEventCoverageDocMatchesManifestFSV (#484) enforces parity. -->

Every WC3 `EVENT_*` constant is accounted for: **136 total**, **45 mapped** to a LitD event kind, **91 tombstoned** (out of deterministic-sim scope, with a reason). Source of truth: `repoes/war3-types/scripts/common.j`.

## By family

| Family | Count |
|---|---|
| dialogevent | 2 |
| gameevent | 16 |
| playerevent | 22 |
| playerunitevent | 47 |
| unitevent | 48 |
| widgetevent | 1 |

## Every event

| EVENT_ | Family | Status | Notes |
|---|---|---|---|
| `EVENT_DIALOG_BUTTON_CLICK` | dialogevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_DIALOG_CLICK` | dialogevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_GAME_BUILD_SUBMENU` | gameevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_GAME_CUSTOM_UI_FRAME` | gameevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_GAME_END_LEVEL` | gameevent | tombstoned | campaign level-flow control; LitD drives level transitions outside the trigger event bus |
| `EVENT_GAME_ENTER_REGION` | gameevent | mapped | mapped to a LitD event kind |
| `EVENT_GAME_LEAVE_REGION` | gameevent | mapped | mapped to a LitD event kind |
| `EVENT_GAME_LOADED` | gameevent | tombstoned | save/load lifecycle is driven by SaveState/LoadState (#456/#464), not a script event |
| `EVENT_GAME_SAVE` | gameevent | tombstoned | save/load lifecycle is driven by SaveState/LoadState (#456/#464), not a script event |
| `EVENT_GAME_SHOW_SKILL` | gameevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_GAME_STATE_LIMIT` | gameevent | tombstoned | GUI float-threshold trigger; LitD has no state-register event surface |
| `EVENT_GAME_TIMER_EXPIRED` | gameevent | tombstoned | timer expiry is delivered via the Timer / Trigger.Every primitive (#464), not a global event constant |
| `EVENT_GAME_TOURNAMENT_FINISH_NOW` | gameevent | tombstoned | Battle.net tournament hook; not applicable to LitD |
| `EVENT_GAME_TOURNAMENT_FINISH_SOON` | gameevent | tombstoned | Battle.net tournament hook; not applicable to LitD |
| `EVENT_GAME_TRACKABLE_HIT` | gameevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_GAME_TRACKABLE_TRACK` | gameevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_GAME_VARIABLE_LIMIT` | gameevent | tombstoned | GUI float-threshold trigger; LitD has no state-register event surface |
| `EVENT_GAME_VICTORY` | gameevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_ALLIANCE_CHANGED` | playerevent | tombstoned | diplomacy/alliance change; LitD alliance model has no event hook yet |
| `EVENT_PLAYER_ARROW_DOWN_DOWN` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_ARROW_DOWN_UP` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_ARROW_LEFT_DOWN` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_ARROW_LEFT_UP` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_ARROW_RIGHT_DOWN` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_ARROW_RIGHT_UP` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_ARROW_UP_DOWN` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_ARROW_UP_UP` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_CHAT` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_DEFEAT` | playerevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_END_CINEMATIC` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_HERO_LEVEL` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_HERO_REVIVABLE` | playerunitevent | tombstoned | hero-revive lifecycle; no LitD revive event yet |
| `EVENT_PLAYER_HERO_REVIVE_CANCEL` | playerunitevent | tombstoned | hero-revive lifecycle; no LitD revive event yet |
| `EVENT_PLAYER_HERO_REVIVE_FINISH` | playerunitevent | tombstoned | hero-revive lifecycle; no LitD revive event yet |
| `EVENT_PLAYER_HERO_REVIVE_START` | playerunitevent | tombstoned | hero-revive lifecycle; no LitD revive event yet |
| `EVENT_PLAYER_HERO_SKILL` | playerunitevent | tombstoned | pending #467/#470: ability-lifecycle events (EvAbilityCast/Effect/Channel/Finish/Stopped) are not modeled yet |
| `EVENT_PLAYER_KEY` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_KEY_DOWN` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_KEY_UP` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_LEAVE` | playerevent | tombstoned | multiplayer-session concern (#326), not a sim event |
| `EVENT_PLAYER_MOUSE_DOWN` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_MOUSE_MOVE` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_MOUSE_UP` | playerevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_STATE_LIMIT` | playerevent | tombstoned | GUI float-threshold trigger; LitD has no state-register event surface |
| `EVENT_PLAYER_SYNC_DATA` | playerevent | tombstoned | multiplayer-session concern (#326), not a sim event |
| `EVENT_PLAYER_UNIT_ATTACKED` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_CHANGE_OWNER` | playerunitevent | tombstoned | ownership transfer; no LitD change-owner event yet |
| `EVENT_PLAYER_UNIT_CONSTRUCT_CANCEL` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_CONSTRUCT_FINISH` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_CONSTRUCT_START` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_DAMAGED` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_DAMAGING` | playerunitevent | tombstoned | pre-mitigation damage is delivered via the OnDamage modifier sink (#406/#475), not the event bus |
| `EVENT_PLAYER_UNIT_DEATH` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_DECAY` | playerunitevent | tombstoned | corpse decay; no LitD decay event |
| `EVENT_PLAYER_UNIT_DESELECTED` | playerunitevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_UNIT_DETECTED` | playerunitevent | tombstoned | detection/true-sight reveal; no LitD detection event |
| `EVENT_PLAYER_UNIT_DROP_ITEM` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_HIDDEN` | playerunitevent | tombstoned | ShowUnit visibility toggle; no LitD event |
| `EVENT_PLAYER_UNIT_ISSUED_ORDER` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_ISSUED_POINT_ORDER` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_ISSUED_TARGET_ORDER` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_ISSUED_UNIT_ORDER` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_LOADED` | playerunitevent | tombstoned | transport load/unload; no LitD transport event |
| `EVENT_PLAYER_UNIT_PAWN_ITEM` | playerunitevent | tombstoned | shop/marketplace transaction; LitD has no shop event surface |
| `EVENT_PLAYER_UNIT_PICKUP_ITEM` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_RESCUED` | playerunitevent | tombstoned | rescuable-unit handoff; no LitD rescue event |
| `EVENT_PLAYER_UNIT_RESEARCH_CANCEL` | playerunitevent | tombstoned | no LitD research-cancel event |
| `EVENT_PLAYER_UNIT_RESEARCH_FINISH` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_RESEARCH_START` | playerunitevent | tombstoned | no LitD research-start event; only research completion is modeled |
| `EVENT_PLAYER_UNIT_SELECTED` | playerunitevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_PLAYER_UNIT_SELL` | playerunitevent | tombstoned | shop/marketplace transaction; LitD has no shop event surface |
| `EVENT_PLAYER_UNIT_SELL_ITEM` | playerunitevent | tombstoned | shop/marketplace transaction; LitD has no shop event surface |
| `EVENT_PLAYER_UNIT_SPELL_CAST` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_SPELL_CHANNEL` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_SPELL_EFFECT` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_SPELL_ENDCAST` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_SPELL_FINISH` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_STACK_ITEM` | playerunitevent | tombstoned | item charge stacking; no LitD event |
| `EVENT_PLAYER_UNIT_SUMMON` | playerunitevent | tombstoned | pending #467/#470: ability-lifecycle events (EvAbilityCast/Effect/Channel/Finish/Stopped) are not modeled yet |
| `EVENT_PLAYER_UNIT_TRAIN_CANCEL` | playerunitevent | tombstoned | no LitD train-cancel event; EventTrainRefused covers refusal-at-issue, not mid-queue cancel |
| `EVENT_PLAYER_UNIT_TRAIN_FINISH` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_UNIT_TRAIN_START` | playerunitevent | tombstoned | no LitD train-start event; only train completion is modeled |
| `EVENT_PLAYER_UNIT_UPGRADE_CANCEL` | playerunitevent | tombstoned | building-upgrade lifecycle; no LitD upgrade event (distinct from research) |
| `EVENT_PLAYER_UNIT_UPGRADE_FINISH` | playerunitevent | tombstoned | building-upgrade lifecycle; no LitD upgrade event (distinct from research) |
| `EVENT_PLAYER_UNIT_UPGRADE_START` | playerunitevent | tombstoned | building-upgrade lifecycle; no LitD upgrade event (distinct from research) |
| `EVENT_PLAYER_UNIT_USE_ITEM` | playerunitevent | mapped | mapped to a LitD event kind |
| `EVENT_PLAYER_VICTORY` | playerevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_ACQUIRED_TARGET` | unitevent | tombstoned | pending #468/#470: attack-lifecycle events (EvAttackLaunch/EvAttackLanded) are not modeled yet |
| `EVENT_UNIT_ATTACKED` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_CHANGE_OWNER` | unitevent | tombstoned | ownership transfer; no LitD change-owner event yet |
| `EVENT_UNIT_CONSTRUCT_CANCEL` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_CONSTRUCT_FINISH` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_DAMAGED` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_DAMAGING` | unitevent | tombstoned | pre-mitigation damage is delivered via the OnDamage modifier sink (#406/#475), not the event bus |
| `EVENT_UNIT_DEATH` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_DECAY` | unitevent | tombstoned | corpse decay; no LitD decay event |
| `EVENT_UNIT_DESELECTED` | unitevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_UNIT_DETECTED` | unitevent | tombstoned | detection/true-sight reveal; no LitD detection event |
| `EVENT_UNIT_DROP_ITEM` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_HERO_LEVEL` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_HERO_REVIVABLE` | unitevent | tombstoned | hero-revive lifecycle; no LitD revive event yet |
| `EVENT_UNIT_HERO_REVIVE_CANCEL` | unitevent | tombstoned | hero-revive lifecycle; no LitD revive event yet |
| `EVENT_UNIT_HERO_REVIVE_FINISH` | unitevent | tombstoned | hero-revive lifecycle; no LitD revive event yet |
| `EVENT_UNIT_HERO_REVIVE_START` | unitevent | tombstoned | hero-revive lifecycle; no LitD revive event yet |
| `EVENT_UNIT_HERO_SKILL` | unitevent | tombstoned | pending #467/#470: ability-lifecycle events (EvAbilityCast/Effect/Channel/Finish/Stopped) are not modeled yet |
| `EVENT_UNIT_HIDDEN` | unitevent | tombstoned | ShowUnit visibility toggle; no LitD event |
| `EVENT_UNIT_ISSUED_ORDER` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_ISSUED_POINT_ORDER` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_ISSUED_TARGET_ORDER` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_LOADED` | unitevent | tombstoned | transport load/unload; no LitD transport event |
| `EVENT_UNIT_PAWN_ITEM` | unitevent | tombstoned | shop/marketplace transaction; LitD has no shop event surface |
| `EVENT_UNIT_PICKUP_ITEM` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_RESCUED` | unitevent | tombstoned | rescuable-unit handoff; no LitD rescue event |
| `EVENT_UNIT_RESEARCH_CANCEL` | unitevent | tombstoned | no LitD research-cancel event |
| `EVENT_UNIT_RESEARCH_FINISH` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_RESEARCH_START` | unitevent | tombstoned | no LitD research-start event; only research completion is modeled |
| `EVENT_UNIT_SELECTED` | unitevent | tombstoned | UI/input concern, out of deterministic-sim scope (render/input/UI layer) |
| `EVENT_UNIT_SELL` | unitevent | tombstoned | shop/marketplace transaction; LitD has no shop event surface |
| `EVENT_UNIT_SELL_ITEM` | unitevent | tombstoned | shop/marketplace transaction; LitD has no shop event surface |
| `EVENT_UNIT_SPELL_CAST` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_SPELL_CHANNEL` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_SPELL_EFFECT` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_SPELL_ENDCAST` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_SPELL_FINISH` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_STACK_ITEM` | unitevent | tombstoned | item charge stacking; no LitD event |
| `EVENT_UNIT_STATE_LIMIT` | unitevent | tombstoned | GUI float-threshold trigger; LitD has no state-register event surface |
| `EVENT_UNIT_SUMMON` | unitevent | tombstoned | pending #467/#470: ability-lifecycle events (EvAbilityCast/Effect/Channel/Finish/Stopped) are not modeled yet |
| `EVENT_UNIT_TARGET_IN_RANGE` | unitevent | tombstoned | proximity trigger; model with a periodic Trigger + distance check, no dedicated LitD event |
| `EVENT_UNIT_TRAIN_CANCEL` | unitevent | tombstoned | no LitD train-cancel event; EventTrainRefused covers refusal-at-issue, not mid-queue cancel |
| `EVENT_UNIT_TRAIN_FINISH` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_UNIT_TRAIN_START` | unitevent | tombstoned | no LitD train-start event; only train completion is modeled |
| `EVENT_UNIT_UPGRADE_CANCEL` | unitevent | tombstoned | building-upgrade lifecycle; no LitD upgrade event (distinct from research) |
| `EVENT_UNIT_UPGRADE_FINISH` | unitevent | tombstoned | building-upgrade lifecycle; no LitD upgrade event (distinct from research) |
| `EVENT_UNIT_UPGRADE_START` | unitevent | tombstoned | building-upgrade lifecycle; no LitD upgrade event (distinct from research) |
| `EVENT_UNIT_USE_ITEM` | unitevent | mapped | mapped to a LitD event kind |
| `EVENT_WIDGET_DEATH` | widgetevent | tombstoned | generic widget death; LitD models destructable death separately, no unified widget event yet |
