package main

// EVENT_ coverage manifest (#466, ADR #451). Every JASS `EVENT_*` constant in
// common.j (families gameevent/playerevent/playerunitevent/unitevent/
// widgetevent/dialogevent) is given a verdict: either MAPPED to a concrete litd
// api.EventKind, or TOMBSTONED with a reason. This turns "as elaborate as WC3"
// into a checked file (docs/api/event-coverage.json) instead of a vibe, applying
// the dedup policy D1–D5 (compression.md) to the event surface.
//
// Dedup note (D2): WC3 exposes most unit events twice — once unit-scoped
// (EVENT_UNIT_*) and once player-scoped (EVENT_PLAYER_UNIT_*). LitD folds the
// scope into the trigger REGISTRATION (TriggerRegisterUnitEvent vs
// TriggerRegisterPlayerUnitEvent both take the same EventKind), so both scope
// variants map to the SAME litd EventKind here — the duplication is made
// explicit per-constant rather than hidden.
//
// Source of truth = common.j. The -eventcov gate fails if any EVENT_ constant is
// neither mapped nor tombstoned, if a tombstone lacks a reason, if a mapped kind
// names a litd EventKind that does not exist, or if the table carries a stale
// entry not present in common.j. Output is deterministic (sorted by name) and
// byte-identical on re-run.

import (
	"encoding/json"
	"fmt"
	"os"
	"regexp"
	"sort"
)

// eventCovOutputPath is where the manifest is written (relative to repo root).
const eventCovOutputPath = "docs/api/event-coverage.json"

// knownEventKinds is the set of litd api.EventKind identifiers a MAPPED row may
// name. It mirrors litd/api/event_payload.go (the 25 kinds defined there); the
// -eventcov check rejects a mapped kind not in this set so the manifest cannot
// drift to a phantom kind. When E2/E3/E4 (#467–#470) add ability/attack/buff
// kinds, add them here and flip the corresponding tombstones to mapped.
var knownEventKinds = map[string]bool{
	"EventUnitDeath": true, "EventUnitDamaged": true, "EventOrderIssued": true,
	"EventOrderDone": true, "EventUnitTrained": true, "EventResearchFinished": true,
	"EventHeroLevel": true, "EventItemPickedUp": true, "EventConstructFinished": true,
	"EventMissileImpact": true, "EventMissileExpired": true, "EventVictory": true,
	"EventDefeat": true, "EventRegionEnter": true, "EventRegionLeave": true,
	"EventOrderDropped": true, "EventBuffExpired": true, "EventResourceDeposited": true,
	"EventResourceDepleted": true, "EventTrainRefused": true, "EventHeroDied": true,
	"EventItemUsed": true, "EventItemDropped": true, "EventConstructStarted": true,
	"EventConstructCancelled": true,
}

// eventVerdict is one constant's coverage decision: exactly one of Kind (mapped)
// or Reason (tombstoned) is set.
type eventVerdict struct {
	Kind   string // litd api.EventKind identifier, "" if tombstoned
	Reason string // tombstone reason, "" if mapped
}

func mapped(kind string) eventVerdict      { return eventVerdict{Kind: kind} }
func tombstone(reason string) eventVerdict { return eventVerdict{Reason: reason} }

// Reason buckets reused across many scope-variant constants so the rationale is
// consistent and a later issue can grep-flip a whole class at once.
const (
	rsnAbility   = "pending #467/#470: ability-lifecycle events (EvAbilityCast/Effect/Channel/Finish/Stopped) are not modeled yet"
	rsnAttack    = "pending #468/#470: attack-lifecycle events (EvAttackLaunch/EvAttackLanded) are not modeled yet"
	rsnDamaging  = "pre-mitigation damage is delivered via the OnDamage modifier sink (#406/#475), not the event bus"
	rsnShop      = "shop/marketplace transaction; LitD has no shop event surface"
	rsnUI        = "UI/input concern, out of deterministic-sim scope (render/input/UI layer)"
	rsnMP        = "multiplayer-session concern (#326), not a sim event"
	rsnRevive    = "hero-revive lifecycle; no LitD revive event yet"
	rsnUpgrade   = "building-upgrade lifecycle; no LitD upgrade event (distinct from research)"
	rsnThreshold = "GUI float-threshold trigger; LitD has no state-register event surface"
)

// eventCoverage is the authored verdict table: JASS EVENT_ constant -> verdict.
// Every constant in common.j must appear here exactly once (validated). Kept in
// readable family groups; output is sorted independently.
var eventCoverage = map[string]eventVerdict{
	// --- gameevent (16) ---
	"EVENT_GAME_VICTORY":               mapped("EventVictory"),
	"EVENT_GAME_ENTER_REGION":          mapped("EventRegionEnter"),
	"EVENT_GAME_LEAVE_REGION":          mapped("EventRegionLeave"),
	"EVENT_GAME_END_LEVEL":             tombstone("campaign level-flow control; LitD drives level transitions outside the trigger event bus"),
	"EVENT_GAME_LOADED":                tombstone("save/load lifecycle is driven by SaveState/LoadState (#456/#464), not a script event"),
	"EVENT_GAME_SAVE":                  tombstone("save/load lifecycle is driven by SaveState/LoadState (#456/#464), not a script event"),
	"EVENT_GAME_TIMER_EXPIRED":         tombstone("timer expiry is delivered via the Timer / Trigger.Every primitive (#464), not a global event constant"),
	"EVENT_GAME_STATE_LIMIT":           tombstone(rsnThreshold),
	"EVENT_GAME_VARIABLE_LIMIT":        tombstone(rsnThreshold),
	"EVENT_GAME_BUILD_SUBMENU":         tombstone(rsnUI),
	"EVENT_GAME_CUSTOM_UI_FRAME":       tombstone(rsnUI),
	"EVENT_GAME_SHOW_SKILL":            tombstone(rsnUI),
	"EVENT_GAME_TRACKABLE_HIT":         tombstone(rsnUI),
	"EVENT_GAME_TRACKABLE_TRACK":       tombstone(rsnUI),
	"EVENT_GAME_TOURNAMENT_FINISH_NOW": tombstone("Battle.net tournament hook; not applicable to LitD"),
	"EVENT_GAME_TOURNAMENT_FINISH_SOON": tombstone("Battle.net tournament hook; not applicable to LitD"),

	// --- playerevent (22) ---
	"EVENT_PLAYER_VICTORY":          mapped("EventVictory"),
	"EVENT_PLAYER_DEFEAT":           mapped("EventDefeat"),
	"EVENT_PLAYER_ALLIANCE_CHANGED": tombstone("diplomacy/alliance change; LitD alliance model has no event hook yet"),
	"EVENT_PLAYER_CHAT":             tombstone(rsnUI),
	"EVENT_PLAYER_LEAVE":            tombstone(rsnMP),
	"EVENT_PLAYER_END_CINEMATIC":    tombstone(rsnUI),
	"EVENT_PLAYER_STATE_LIMIT":      tombstone(rsnThreshold),
	"EVENT_PLAYER_SYNC_DATA":        tombstone(rsnMP),
	"EVENT_PLAYER_ARROW_DOWN_DOWN":  tombstone(rsnUI),
	"EVENT_PLAYER_ARROW_DOWN_UP":    tombstone(rsnUI),
	"EVENT_PLAYER_ARROW_LEFT_DOWN":  tombstone(rsnUI),
	"EVENT_PLAYER_ARROW_LEFT_UP":    tombstone(rsnUI),
	"EVENT_PLAYER_ARROW_RIGHT_DOWN": tombstone(rsnUI),
	"EVENT_PLAYER_ARROW_RIGHT_UP":   tombstone(rsnUI),
	"EVENT_PLAYER_ARROW_UP_DOWN":    tombstone(rsnUI),
	"EVENT_PLAYER_ARROW_UP_UP":      tombstone(rsnUI),
	"EVENT_PLAYER_KEY":              tombstone(rsnUI),
	"EVENT_PLAYER_KEY_DOWN":         tombstone(rsnUI),
	"EVENT_PLAYER_KEY_UP":           tombstone(rsnUI),
	"EVENT_PLAYER_MOUSE_DOWN":       tombstone(rsnUI),
	"EVENT_PLAYER_MOUSE_UP":         tombstone(rsnUI),
	"EVENT_PLAYER_MOUSE_MOVE":       tombstone(rsnUI),

	// --- playerunitevent (47) ---
	"EVENT_PLAYER_HERO_LEVEL":               mapped("EventHeroLevel"),
	"EVENT_PLAYER_UNIT_DEATH":               mapped("EventUnitDeath"),
	"EVENT_PLAYER_UNIT_DAMAGED":             mapped("EventUnitDamaged"),
	"EVENT_PLAYER_UNIT_ISSUED_ORDER":        mapped("EventOrderIssued"),
	"EVENT_PLAYER_UNIT_ISSUED_POINT_ORDER":  mapped("EventOrderIssued"),
	"EVENT_PLAYER_UNIT_ISSUED_TARGET_ORDER": mapped("EventOrderIssued"),
	"EVENT_PLAYER_UNIT_ISSUED_UNIT_ORDER":   mapped("EventOrderIssued"),
	"EVENT_PLAYER_UNIT_TRAIN_FINISH":        mapped("EventUnitTrained"),
	"EVENT_PLAYER_UNIT_RESEARCH_FINISH":     mapped("EventResearchFinished"),
	"EVENT_PLAYER_UNIT_CONSTRUCT_FINISH":    mapped("EventConstructFinished"),
	"EVENT_PLAYER_UNIT_CONSTRUCT_CANCEL":    mapped("EventConstructCancelled"),
	"EVENT_PLAYER_UNIT_CONSTRUCT_START":     mapped("EventConstructStarted"),
	"EVENT_PLAYER_UNIT_PICKUP_ITEM":         mapped("EventItemPickedUp"),
	"EVENT_PLAYER_UNIT_USE_ITEM":            mapped("EventItemUsed"),
	"EVENT_PLAYER_UNIT_DROP_ITEM":           mapped("EventItemDropped"),
	"EVENT_PLAYER_HERO_SKILL":               tombstone(rsnAbility),
	"EVENT_PLAYER_HERO_REVIVABLE":           tombstone(rsnRevive),
	"EVENT_PLAYER_HERO_REVIVE_START":        tombstone(rsnRevive),
	"EVENT_PLAYER_HERO_REVIVE_CANCEL":       tombstone(rsnRevive),
	"EVENT_PLAYER_HERO_REVIVE_FINISH":       tombstone(rsnRevive),
	"EVENT_PLAYER_UNIT_ATTACKED":            tombstone(rsnAttack),
	"EVENT_PLAYER_UNIT_DAMAGING":            tombstone(rsnDamaging),
	"EVENT_PLAYER_UNIT_SPELL_CAST":          tombstone(rsnAbility),
	"EVENT_PLAYER_UNIT_SPELL_CHANNEL":       tombstone(rsnAbility),
	"EVENT_PLAYER_UNIT_SPELL_EFFECT":        tombstone(rsnAbility),
	"EVENT_PLAYER_UNIT_SPELL_ENDCAST":       tombstone(rsnAbility),
	"EVENT_PLAYER_UNIT_SPELL_FINISH":        tombstone(rsnAbility),
	"EVENT_PLAYER_UNIT_SUMMON":              tombstone(rsnAbility),
	"EVENT_PLAYER_UNIT_TRAIN_START":         tombstone("no LitD train-start event; only train completion is modeled"),
	"EVENT_PLAYER_UNIT_TRAIN_CANCEL":        tombstone("no LitD train-cancel event; EventTrainRefused covers refusal-at-issue, not mid-queue cancel"),
	"EVENT_PLAYER_UNIT_RESEARCH_START":      tombstone("no LitD research-start event; only research completion is modeled"),
	"EVENT_PLAYER_UNIT_RESEARCH_CANCEL":     tombstone("no LitD research-cancel event"),
	"EVENT_PLAYER_UNIT_UPGRADE_START":       tombstone(rsnUpgrade),
	"EVENT_PLAYER_UNIT_UPGRADE_FINISH":      tombstone(rsnUpgrade),
	"EVENT_PLAYER_UNIT_UPGRADE_CANCEL":      tombstone(rsnUpgrade),
	"EVENT_PLAYER_UNIT_SELL":                tombstone(rsnShop),
	"EVENT_PLAYER_UNIT_SELL_ITEM":           tombstone(rsnShop),
	"EVENT_PLAYER_UNIT_PAWN_ITEM":           tombstone(rsnShop),
	"EVENT_PLAYER_UNIT_STACK_ITEM":          tombstone("item charge stacking; no LitD event"),
	"EVENT_PLAYER_UNIT_CHANGE_OWNER":        tombstone("ownership transfer; no LitD change-owner event yet"),
	"EVENT_PLAYER_UNIT_DECAY":               tombstone("corpse decay; no LitD decay event"),
	"EVENT_PLAYER_UNIT_SELECTED":            tombstone(rsnUI),
	"EVENT_PLAYER_UNIT_DESELECTED":          tombstone(rsnUI),
	"EVENT_PLAYER_UNIT_DETECTED":            tombstone("detection/true-sight reveal; no LitD detection event"),
	"EVENT_PLAYER_UNIT_HIDDEN":              tombstone("ShowUnit visibility toggle; no LitD event"),
	"EVENT_PLAYER_UNIT_LOADED":              tombstone("transport load/unload; no LitD transport event"),
	"EVENT_PLAYER_UNIT_RESCUED":             tombstone("rescuable-unit handoff; no LitD rescue event"),

	// --- unitevent (48) ---
	"EVENT_UNIT_DEATH":               mapped("EventUnitDeath"),
	"EVENT_UNIT_DAMAGED":             mapped("EventUnitDamaged"),
	"EVENT_UNIT_ISSUED_ORDER":        mapped("EventOrderIssued"),
	"EVENT_UNIT_ISSUED_POINT_ORDER":  mapped("EventOrderIssued"),
	"EVENT_UNIT_ISSUED_TARGET_ORDER": mapped("EventOrderIssued"),
	"EVENT_UNIT_TRAIN_FINISH":        mapped("EventUnitTrained"),
	"EVENT_UNIT_RESEARCH_FINISH":     mapped("EventResearchFinished"),
	"EVENT_UNIT_CONSTRUCT_FINISH":    mapped("EventConstructFinished"),
	"EVENT_UNIT_CONSTRUCT_CANCEL":    mapped("EventConstructCancelled"),
	"EVENT_UNIT_PICKUP_ITEM":         mapped("EventItemPickedUp"),
	"EVENT_UNIT_USE_ITEM":            mapped("EventItemUsed"),
	"EVENT_UNIT_DROP_ITEM":           mapped("EventItemDropped"),
	"EVENT_UNIT_HERO_LEVEL":          mapped("EventHeroLevel"),
	"EVENT_UNIT_ATTACKED":            tombstone(rsnAttack),
	"EVENT_UNIT_ACQUIRED_TARGET":     tombstone(rsnAttack),
	"EVENT_UNIT_TARGET_IN_RANGE":     tombstone("proximity trigger; model with a periodic Trigger + distance check, no dedicated LitD event"),
	"EVENT_UNIT_DAMAGING":            tombstone(rsnDamaging),
	"EVENT_UNIT_SPELL_CAST":          tombstone(rsnAbility),
	"EVENT_UNIT_SPELL_CHANNEL":       tombstone(rsnAbility),
	"EVENT_UNIT_SPELL_EFFECT":        tombstone(rsnAbility),
	"EVENT_UNIT_SPELL_ENDCAST":       tombstone(rsnAbility),
	"EVENT_UNIT_SPELL_FINISH":        tombstone(rsnAbility),
	"EVENT_UNIT_SUMMON":              tombstone(rsnAbility),
	"EVENT_UNIT_HERO_SKILL":          tombstone(rsnAbility),
	"EVENT_UNIT_HERO_REVIVABLE":      tombstone(rsnRevive),
	"EVENT_UNIT_HERO_REVIVE_START":   tombstone(rsnRevive),
	"EVENT_UNIT_HERO_REVIVE_CANCEL":  tombstone(rsnRevive),
	"EVENT_UNIT_HERO_REVIVE_FINISH":  tombstone(rsnRevive),
	"EVENT_UNIT_TRAIN_START":         tombstone("no LitD train-start event; only train completion is modeled"),
	"EVENT_UNIT_TRAIN_CANCEL":        tombstone("no LitD train-cancel event; EventTrainRefused covers refusal-at-issue, not mid-queue cancel"),
	"EVENT_UNIT_RESEARCH_START":      tombstone("no LitD research-start event; only research completion is modeled"),
	"EVENT_UNIT_RESEARCH_CANCEL":     tombstone("no LitD research-cancel event"),
	"EVENT_UNIT_UPGRADE_START":       tombstone(rsnUpgrade),
	"EVENT_UNIT_UPGRADE_FINISH":      tombstone(rsnUpgrade),
	"EVENT_UNIT_UPGRADE_CANCEL":      tombstone(rsnUpgrade),
	"EVENT_UNIT_SELL":                tombstone(rsnShop),
	"EVENT_UNIT_SELL_ITEM":           tombstone(rsnShop),
	"EVENT_UNIT_PAWN_ITEM":           tombstone(rsnShop),
	"EVENT_UNIT_STACK_ITEM":          tombstone("item charge stacking; no LitD event"),
	"EVENT_UNIT_CHANGE_OWNER":        tombstone("ownership transfer; no LitD change-owner event yet"),
	"EVENT_UNIT_DECAY":               tombstone("corpse decay; no LitD decay event"),
	"EVENT_UNIT_SELECTED":            tombstone(rsnUI),
	"EVENT_UNIT_DESELECTED":          tombstone(rsnUI),
	"EVENT_UNIT_DETECTED":            tombstone("detection/true-sight reveal; no LitD detection event"),
	"EVENT_UNIT_HIDDEN":              tombstone("ShowUnit visibility toggle; no LitD event"),
	"EVENT_UNIT_LOADED":              tombstone("transport load/unload; no LitD transport event"),
	"EVENT_UNIT_RESCUED":             tombstone("rescuable-unit handoff; no LitD rescue event"),
	"EVENT_UNIT_STATE_LIMIT":         tombstone(rsnThreshold),

	// --- widgetevent (1) ---
	"EVENT_WIDGET_DEATH": tombstone("generic widget death; LitD models destructable death separately, no unified widget event yet"),

	// --- dialogevent (2) ---
	"EVENT_DIALOG_BUTTON_CLICK": tombstone(rsnUI),
	"EVENT_DIALOG_CLICK":        tombstone(rsnUI),
}

// eventCovRow is one serialized manifest row.
type eventCovRow struct {
	Name   string `json:"name"`
	Family string `json:"family"`
	Raw    int    `json:"raw"`
	Status string `json:"status"`           // "mapped" | "tombstoned"
	Kind   string `json:"kind,omitempty"`   // litd EventKind, mapped only
	Reason string `json:"reason,omitempty"` // tombstone reason, tombstoned only
}

// eventCovManifest is the serialized artifact.
type eventCovManifest struct {
	GeneratedFrom string         `json:"generatedFrom"`
	Total         int            `json:"total"`
	Mapped        int            `json:"mapped"`
	Tombstoned    int            `json:"tombstoned"`
	Families      map[string]int `json:"families"`
	Events        []eventCovRow  `json:"events"`
}

// eventConstRe matches a `constant <eventtype> EVENT_NAME = Convert<X>Event(N)`
// global declaration in common.j.
var eventConstRe = regexp.MustCompile(
	`constant\s+(gameevent|playerevent|playerunitevent|unitevent|widgetevent|dialogevent)\s+(EVENT_[A-Z0-9_]+)\s*=\s*Convert\w+\((\d+)\)`)

// parsedEventConst is one EVENT_ constant scanned from common.j.
type parsedEventConst struct {
	name   string
	family string
	raw    int
}

// parseEventConstants extracts every EVENT_ constant declaration from common.j
// source text, in source order.
func parseEventConstants(src string) []parsedEventConst {
	ms := eventConstRe.FindAllStringSubmatch(src, -1)
	out := make([]parsedEventConst, 0, len(ms))
	for _, m := range ms {
		raw := 0
		fmt.Sscanf(m[3], "%d", &raw)
		out = append(out, parsedEventConst{family: m[1], name: m[2], raw: raw})
	}
	return out
}

// buildEventCoverage parses common.j and joins it against the authored verdict
// table. See buildEventCoverageWith.
func buildEventCoverage(src string) (eventCovManifest, []error) {
	return buildEventCoverageWith(src, eventCoverage)
}

// buildEventCoverageWith parses the constants and joins them against the given
// verdict table, returning the manifest plus a list of validation errors (empty
// on success). It is fail-closed: an unaccounted constant, a missing reason, a
// phantom mapped kind, or a stale table entry each yields an error. The table is
// a parameter so tests can drive the edge cases against synthetic inputs.
func buildEventCoverageWith(src string, table map[string]eventVerdict) (eventCovManifest, []error) {
	consts := parseEventConstants(src)
	var errs []error

	seen := map[string]bool{}
	rows := make([]eventCovRow, 0, len(consts))
	families := map[string]int{}
	var nMapped, nTomb int

	for _, c := range consts {
		if seen[c.name] {
			errs = append(errs, fmt.Errorf("duplicate EVENT_ constant in common.j: %s", c.name))
			continue
		}
		seen[c.name] = true
		families[c.family]++

		v, ok := table[c.name]
		if !ok {
			errs = append(errs, fmt.Errorf("%s (%s) is neither mapped nor tombstoned — add a verdict to eventCoverage", c.name, c.family))
			continue
		}
		row := eventCovRow{Name: c.name, Family: c.family, Raw: c.raw}
		switch {
		case v.Kind != "" && v.Reason != "":
			errs = append(errs, fmt.Errorf("%s has BOTH a mapped kind and a tombstone reason — exactly one allowed", c.name))
			continue
		case v.Kind != "":
			if !knownEventKinds[v.Kind] {
				errs = append(errs, fmt.Errorf("%s maps to %q which is not a known litd EventKind (see knownEventKinds)", c.name, v.Kind))
				continue
			}
			row.Status, row.Kind = "mapped", v.Kind
			nMapped++
		case v.Reason != "":
			row.Status, row.Reason = "tombstoned", v.Reason
			nTomb++
		default:
			errs = append(errs, fmt.Errorf("%s has an empty verdict — set a mapped kind or a tombstone reason", c.name))
			continue
		}
		rows = append(rows, row)
	}

	// Stale-entry guard: every authored verdict must correspond to a real
	// constant in common.j, else the table has drifted.
	for name := range table {
		if !seen[name] {
			errs = append(errs, fmt.Errorf("eventCoverage has a stale entry %q not present in common.j", name))
		}
	}

	sort.Slice(rows, func(i, j int) bool { return rows[i].Name < rows[j].Name })
	m := eventCovManifest{
		GeneratedFrom: "repoes/war3-types/scripts/common.j",
		Total:         len(rows),
		Mapped:        nMapped,
		Tombstoned:    nTomb,
		Families:      families,
		Events:        rows,
	}
	return m, errs
}

// marshalEventCov renders the manifest deterministically (sorted keys, trailing
// newline) so re-runs are byte-identical.
func marshalEventCov(m eventCovManifest) []byte {
	b, err := json.MarshalIndent(m, "", "  ")
	if err != nil {
		fatal(err)
	}
	return append(b, '\n')
}

// runEventCov is the -eventcov subcommand: parse common.j, build + validate the
// coverage manifest, write docs/api/event-coverage.json, and exit nonzero on any
// validation error (fail-closed). Stderr prints the family counts and tallies.
func runEventCov() {
	src := string(mustRead(defaultScriptsDir + "/common.j"))
	m, errs := buildEventCoverage(src)
	if len(errs) > 0 {
		fmt.Fprintf(os.Stderr, "eventcov: %d validation error(s):\n", len(errs))
		for _, e := range errs {
			fmt.Fprintln(os.Stderr, "  -", e)
		}
		os.Exit(1)
	}
	if err := os.WriteFile(eventCovOutputPath, marshalEventCov(m), 0o644); err != nil {
		fatal(err)
	}
	fmt.Fprintf(os.Stderr, "eventcov: total=%d mapped=%d tombstoned=%d\n", m.Total, m.Mapped, m.Tombstoned)
	fmt.Fprintf(os.Stderr, "eventcov: families gameevent=%d playerevent=%d playerunitevent=%d unitevent=%d widgetevent=%d dialogevent=%d\n",
		m.Families["gameevent"], m.Families["playerevent"], m.Families["playerunitevent"],
		m.Families["unitevent"], m.Families["widgetevent"], m.Families["dialogevent"])
	fmt.Fprintln(os.Stderr, "eventcov: GREEN (every EVENT_ accounted; 0 unmapped-untombstoned)")
}
