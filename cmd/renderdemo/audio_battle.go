package main

import (
	"fmt"
	"math"
	"testing"

	litaudio "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
)

const (
	battle500Cols  = 25
	battle500Rows  = 20
	battle500Units = battle500Cols * battle500Rows
)

type voiceBattleRuntimeDump struct {
	Scene              string                    `json:"scene"`
	Units              int                       `json:"units"`
	Columns            int                       `json:"columns"`
	Rows               int                       `json:"rows"`
	MaxWorldVoices     int                       `json:"maxWorldVoices"`
	MaxUIVoices        int                       `json:"maxUiVoices"`
	MaxStreamVoices    int                       `json:"maxStreamVoices"`
	MaxTotalVoices     int                       `json:"maxTotalVoices"`
	Volley             voiceVolleyDump           `json:"volley"`
	UI                 voiceUIAdmissionDump      `json:"ui"`
	Alert              voiceStealDump            `json:"alert"`
	EqualPriority      voiceEqualPriorityDump    `json:"equalPriority"`
	DistanceCull       voiceCullDump             `json:"distanceCull"`
	LateRetrigger      voiceLateRetriggerDump    `json:"lateRetrigger"`
	Budget             voiceBudgetDump           `json:"budget"`
	AllocsPerRun       float64                   `json:"allocsPerRun"`
	ZeroAlloc          bool                      `json:"zeroAlloc"`
	Events             []voiceAdmissionEventDump `json:"events"`
	VoiceCountTrace    []voiceCountTraceDump     `json:"voiceCountTrace"`
	FinalSlots         []voiceSlotDump           `json:"finalSlots"`
	AudibleDescription string                    `json:"audibleDescription"`
	OK                 bool                      `json:"ok"`
	Errors             []string                  `json:"errors,omitempty"`
}

type voiceVolleyDump struct {
	Asset           uint32  `json:"asset"`
	Events          int     `json:"events"`
	Admitted        int     `json:"admitted"`
	Coalesced       int     `json:"coalesced"`
	ActiveInstances int     `json:"activeInstances"`
	MergedGain      float64 `json:"mergedGain"`
	OK              bool    `json:"ok"`
}

type voiceUIAdmissionDump struct {
	WorldBefore int    `json:"worldBefore"`
	UIAfter     int    `json:"uiAfter"`
	Outcome     string `json:"outcome"`
	OK          bool   `json:"ok"`
}

type voiceStealDump struct {
	Outcome        string  `json:"outcome"`
	Slot           int     `json:"slot"`
	Victim         int     `json:"victim"`
	VictimCue      uint32  `json:"victimCue"`
	VictimPriority string  `json:"victimPriority"`
	VictimDistance float64 `json:"victimDistance"`
	WorldAfter     int     `json:"worldAfter"`
	FadeMs         int     `json:"fadeMs"`
	OK             bool    `json:"ok"`
}

type voiceEqualPriorityDump struct {
	NearOutcome  string  `json:"nearOutcome"`
	NearVictim   int     `json:"nearVictim"`
	NearDistance float64 `json:"nearDistance"`
	FarOutcome   string  `json:"farOutcome"`
	FarDistance  float64 `json:"farDistance"`
	OK           bool    `json:"ok"`
}

type voiceCullDump struct {
	Outcome     string  `json:"outcome"`
	Distance    float64 `json:"distance"`
	MaxAudible  float64 `json:"maxAudible"`
	ActiveAfter int     `json:"activeAfter"`
	OK          bool    `json:"ok"`
}

type voiceLateRetriggerDump struct {
	InsideTimeMs int64  `json:"insideTimeMs"`
	Inside       string `json:"inside"`
	LateTimeMs   int64  `json:"lateTimeMs"`
	Late         string `json:"late"`
	ActiveAfter  int    `json:"activeAfter"`
	OK           bool   `json:"ok"`
}

type voiceBudgetDump struct {
	World        int    `json:"world"`
	UI           int    `json:"ui"`
	Stream       int    `json:"stream"`
	Total        int    `json:"total"`
	ExtraOutcome string `json:"extraOutcome"`
	OK           bool   `json:"ok"`
}

type voiceAdmissionEventDump struct {
	Case           string              `json:"case"`
	Event          string              `json:"event"`
	Rule           string              `json:"rule"`
	Outcome        string              `json:"outcome"`
	Cue            uint32              `json:"cue"`
	Asset          uint32              `json:"asset"`
	Partition      string              `json:"partition"`
	Priority       string              `json:"priority"`
	TimeMs         int64               `json:"timeMs"`
	Distance       float64             `json:"distance"`
	Slot           int                 `json:"slot"`
	Victim         int                 `json:"victim"`
	VictimCue      uint32              `json:"victimCue,omitempty"`
	VictimPriority string              `json:"victimPriority,omitempty"`
	VictimDistance float64             `json:"victimDistance,omitempty"`
	GainBump       float64             `json:"gainBump,omitempty"`
	Counts         voiceCountTraceDump `json:"counts"`
}

type voiceCountTraceDump struct {
	Event  string `json:"event"`
	World  int    `json:"world"`
	UI     int    `json:"ui"`
	Stream int    `json:"stream"`
	Total  int    `json:"total"`
}

type voiceSlotDump struct {
	Slot      int     `json:"slot"`
	Active    bool    `json:"active"`
	Cue       uint32  `json:"cue,omitempty"`
	Asset     uint32  `json:"asset,omitempty"`
	Partition string  `json:"partition,omitempty"`
	Priority  string  `json:"priority,omitempty"`
	Distance  float64 `json:"distance,omitempty"`
	Gain      float64 `json:"gain,omitempty"`
}

func buildBattle500FSV(scene *core.Node) (sceneSpec, *voiceBattleRuntimeDump, error) {
	drawBattle500Scene(scene)
	dump := runBattle500VoiceFSV()
	return sceneSpec{name: "battle500", expected: expectedStats(battle500Units+1, 0, battle500Units+1, 0, 4, 0)}, dump, nil
}

func drawBattle500Scene(scene *core.Node) {
	groundMat := material.NewStandard(&math32.Color{R: 0.10, G: 0.16, B: 0.12})
	ground := graphic.NewMesh(geometry.NewPlane(1100, 880), groundMat)
	ground.SetRotationX(-math32.Pi / 2)
	scene.Add(ground)

	geom := geometry.NewBox(22, 30, 22)
	blue := material.NewStandard(&math32.Color{R: 0.22, G: 0.43, B: 0.95})
	red := material.NewStandard(&math32.Color{R: 0.92, G: 0.22, B: 0.18})
	const spacing = float32(34)
	x0 := -float32(battle500Cols-1) * spacing * 0.5
	z0 := -float32(battle500Rows-1) * spacing * 0.5
	for row := 0; row < battle500Rows; row++ {
		for col := 0; col < battle500Cols; col++ {
			mat := blue
			if row >= battle500Rows/2 {
				mat = red
			}
			addMesh(scene, geom, mat, x0+float32(col)*spacing, 15, z0+float32(row)*spacing)
		}
	}
}

func runBattle500VoiceFSV() *voiceBattleRuntimeDump {
	d := &voiceBattleRuntimeDump{
		Scene:              "battle500",
		Units:              battle500Units,
		Columns:            battle500Cols,
		Rows:               battle500Rows,
		OK:                 true,
		AudibleDescription: "Expected on hardware: the 40-impact volley starts only three impact instances; later impacts thicken one playing voice through capped gain bump instead of retriggering 40 starts, so the result is a dense hit cluster without machine-gun restart chatter.",
	}
	runBattleVolleyCase(d)
	runBattleSaturationCase(d)
	runBattleEqualPriorityCase(d)
	runBattleCullCase(d)
	runBattleLateRetriggerCase(d)
	runBattleBudgetCase(d)
	d.AllocsPerRun = battleAdmissionAllocsPerRun()
	d.ZeroAlloc = d.AllocsPerRun == 0
	if !d.ZeroAlloc {
		d.Errors = append(d.Errors, fmt.Sprintf("Admit allocated %.3f objects/op", d.AllocsPerRun))
	}
	if d.MaxWorldVoices > litaudio.WorldVoices {
		d.Errors = append(d.Errors, fmt.Sprintf("world voices exceeded %d: %d", litaudio.WorldVoices, d.MaxWorldVoices))
	}
	if d.MaxUIVoices > litaudio.UIVoices {
		d.Errors = append(d.Errors, fmt.Sprintf("UI voices exceeded %d: %d", litaudio.UIVoices, d.MaxUIVoices))
	}
	if d.MaxStreamVoices > litaudio.StreamVoices {
		d.Errors = append(d.Errors, fmt.Sprintf("stream voices exceeded %d: %d", litaudio.StreamVoices, d.MaxStreamVoices))
	}
	if d.MaxTotalVoices > litaudio.TotalVoices {
		d.Errors = append(d.Errors, fmt.Sprintf("total voices exceeded %d: %d", litaudio.TotalVoices, d.MaxTotalVoices))
	}
	d.OK = d.OK && len(d.Errors) == 0
	return d
}

func runBattleVolleyCase(d *voiceBattleRuntimeDump) {
	a := litaudio.NewAllocator(litaudio.FalloffRadius)
	const (
		asset = uint32(700)
		total = 40
	)
	admitted, coalesced := 0, 0
	for i := 0; i < total; i++ {
		dec := recordBattleAdmit(d, a, "40-impact-volley", fmt.Sprintf("impact-%02d", i), litaudio.VoiceRequest{
			Cue: asset, Asset: asset, Partition: litaudio.PartitionWorld, Priority: litaudio.PrioAttackImpact,
			HasPos: true, Pos: litaudio.Vec3{X: float64(i % 8)}, Volume: 0.5, TimeMs: int64(i % litaudio.RetriggerWindowMs),
		})
		switch dec.Outcome {
		case litaudio.Admitted:
			admitted++
		case litaudio.Coalesced:
			coalesced++
		}
	}
	mergedGain := maxSlotGain(a)
	d.Volley = voiceVolleyDump{
		Asset:           asset,
		Events:          total,
		Admitted:        admitted,
		Coalesced:       coalesced,
		ActiveInstances: a.ActiveIn(litaudio.PartitionWorld),
		MergedGain:      mergedGain,
		OK: admitted == litaudio.MaxConcurrentPerAsset &&
			coalesced == total-litaudio.MaxConcurrentPerAsset &&
			a.ActiveIn(litaudio.PartitionWorld) == litaudio.MaxConcurrentPerAsset &&
			mergedGain <= 0.5+litaudio.CoalesceGainCap+1e-9,
	}
	if !d.Volley.OK {
		d.Errors = append(d.Errors, fmt.Sprintf("volley mismatch: %+v", d.Volley))
	}
	d.FinalSlots = battleSlotSnapshot(a)
}

func runBattleSaturationCase(d *voiceBattleRuntimeDump) {
	a := litaudio.NewAllocator(litaudio.FalloffRadius)
	for i := 0; i < litaudio.WorldVoices; i++ {
		recordBattleAdmit(d, a, "saturated-battle", fmt.Sprintf("ambient-fill-%02d", i), litaudio.VoiceRequest{
			Cue: uint32(900 + i), Asset: uint32(900 + i), Partition: litaudio.PartitionWorld,
			Priority: litaudio.PrioAmbient, HasPos: true, Pos: litaudio.Vec3{X: float64(50 + i)}, Volume: 1, TimeMs: int64(i),
		})
	}
	worldBefore := a.ActiveIn(litaudio.PartitionWorld)
	ui := recordBattleAdmit(d, a, "saturated-battle", "ui-click-under-fire", litaudio.VoiceRequest{
		Cue: 1001, Asset: 1001, Partition: litaudio.PartitionUI, Priority: litaudio.PrioAlert, Volume: 1, TimeMs: 60,
	})
	d.UI = voiceUIAdmissionDump{
		WorldBefore: worldBefore,
		UIAfter:     a.ActiveIn(litaudio.PartitionUI),
		Outcome:     ui.Outcome.String(),
		OK:          worldBefore == litaudio.WorldVoices && ui.Outcome == litaudio.Admitted && a.ActiveIn(litaudio.PartitionUI) == 1,
	}
	if !d.UI.OK {
		d.Errors = append(d.Errors, fmt.Sprintf("UI admission mismatch: %+v", d.UI))
	}
	before := battleSlotSnapshot(a)
	alert := recordBattleAdmit(d, a, "saturated-battle", "alert-steals-weakest", litaudio.VoiceRequest{
		Cue: 2001, Asset: 2001, Partition: litaudio.PartitionWorld, Priority: litaudio.PrioAlert,
		HasPos: true, Pos: litaudio.Vec3{X: 1}, Volume: 1, TimeMs: 61,
	})
	victim := voiceSlotDump{}
	if alert.Victim >= 0 && alert.Victim < len(before) {
		victim = before[alert.Victim]
	}
	d.Alert = voiceStealDump{
		Outcome:        alert.Outcome.String(),
		Slot:           alert.Slot,
		Victim:         alert.Victim,
		VictimCue:      victim.Cue,
		VictimPriority: victim.Priority,
		VictimDistance: victim.Distance,
		WorldAfter:     a.ActiveIn(litaudio.PartitionWorld),
		FadeMs:         litaudio.FadeMs,
		OK: alert.Outcome == litaudio.Stolen &&
			alert.Victim >= 0 &&
			victim.Priority == priorityName(litaudio.PrioAmbient) &&
			a.ActiveIn(litaudio.PartitionWorld) == litaudio.WorldVoices,
	}
	if !d.Alert.OK {
		d.Errors = append(d.Errors, fmt.Sprintf("alert steal mismatch: %+v", d.Alert))
	}
}

func runBattleEqualPriorityCase(d *voiceBattleRuntimeDump) {
	a := litaudio.NewAllocator(litaudio.FalloffRadius)
	for i := 0; i < litaudio.WorldVoices; i++ {
		recordBattleAdmit(d, a, "equal-priority", fmt.Sprintf("far-fill-%02d", i), litaudio.VoiceRequest{
			Cue: uint32(3000 + i), Asset: uint32(3000 + i), Partition: litaudio.PartitionWorld,
			Priority: litaudio.PrioAmbient, HasPos: true, Pos: litaudio.Vec3{X: 1000}, Volume: 1, TimeMs: int64(i),
		})
	}
	near := recordBattleAdmit(d, a, "equal-priority", "near-footstep-wins", litaudio.VoiceRequest{
		Cue: 4001, Asset: 4001, Partition: litaudio.PartitionWorld,
		Priority: litaudio.PrioAmbient, HasPos: true, Pos: litaudio.Vec3{X: 10}, Volume: 1, TimeMs: 80,
	})
	far := recordBattleAdmit(d, a, "equal-priority", "far-footstep-drops", litaudio.VoiceRequest{
		Cue: 4002, Asset: 4002, Partition: litaudio.PartitionWorld,
		Priority: litaudio.PrioAmbient, HasPos: true, Pos: litaudio.Vec3{X: 1100}, Volume: 1, TimeMs: 81,
	})
	d.EqualPriority = voiceEqualPriorityDump{
		NearOutcome:  near.Outcome.String(),
		NearVictim:   near.Victim,
		NearDistance: 10,
		FarOutcome:   far.Outcome.String(),
		FarDistance:  1100,
		OK:           near.Outcome == litaudio.Stolen && far.Outcome == litaudio.Dropped,
	}
	if !d.EqualPriority.OK {
		d.Errors = append(d.Errors, fmt.Sprintf("equal-priority mismatch: %+v", d.EqualPriority))
	}
}

func runBattleCullCase(d *voiceBattleRuntimeDump) {
	a := litaudio.NewAllocator(litaudio.FalloffRadius)
	dist := litaudio.FalloffRadius + 800
	cull := recordBattleAdmit(d, a, "distance-cull", "offscreen-war-hit", litaudio.VoiceRequest{
		Cue: 5001, Asset: 5001, Partition: litaudio.PartitionWorld,
		Priority: litaudio.PrioAttackImpact, HasPos: true, Pos: litaudio.Vec3{X: dist}, Volume: 1, TimeMs: 0,
	})
	d.DistanceCull = voiceCullDump{
		Outcome:     cull.Outcome.String(),
		Distance:    dist,
		MaxAudible:  litaudio.FalloffRadius,
		ActiveAfter: a.ActiveTotal(),
		OK:          cull.Outcome == litaudio.CulledDistance && a.ActiveTotal() == 0,
	}
	if !d.DistanceCull.OK {
		d.Errors = append(d.Errors, fmt.Sprintf("distance cull mismatch: %+v", d.DistanceCull))
	}
}

func runBattleLateRetriggerCase(d *voiceBattleRuntimeDump) {
	a := litaudio.NewAllocator(litaudio.FalloffRadius)
	for i := 0; i < litaudio.MaxConcurrentPerAsset; i++ {
		recordBattleAdmit(d, a, "late-retrigger", fmt.Sprintf("same-asset-start-%02d", i), litaudio.VoiceRequest{
			Cue: 6001, Asset: 6001, Partition: litaudio.PartitionWorld,
			Priority: litaudio.PrioAttackImpact, HasPos: true, Pos: litaudio.Vec3{}, Volume: 0.4, TimeMs: int64(i),
		})
	}
	inside := recordBattleAdmit(d, a, "late-retrigger", "inside-window-overflow", litaudio.VoiceRequest{
		Cue: 6001, Asset: 6001, Partition: litaudio.PartitionWorld,
		Priority: litaudio.PrioAttackImpact, HasPos: true, Pos: litaudio.Vec3{}, Volume: 0.4, TimeMs: litaudio.RetriggerWindowMs,
	})
	late := recordBattleAdmit(d, a, "late-retrigger", "late-window-overflow", litaudio.VoiceRequest{
		Cue: 6001, Asset: 6001, Partition: litaudio.PartitionWorld,
		Priority: litaudio.PrioAttackImpact, HasPos: true, Pos: litaudio.Vec3{}, Volume: 0.4, TimeMs: int64(litaudio.MaxConcurrentPerAsset-1) + litaudio.RetriggerWindowMs + 1,
	})
	d.LateRetrigger = voiceLateRetriggerDump{
		InsideTimeMs: litaudio.RetriggerWindowMs,
		Inside:       inside.Outcome.String(),
		LateTimeMs:   int64(litaudio.MaxConcurrentPerAsset-1) + litaudio.RetriggerWindowMs + 1,
		Late:         late.Outcome.String(),
		ActiveAfter:  a.ActiveIn(litaudio.PartitionWorld),
		OK:           inside.Outcome == litaudio.Coalesced && late.Outcome == litaudio.Dropped && a.ActiveIn(litaudio.PartitionWorld) == litaudio.MaxConcurrentPerAsset,
	}
	if !d.LateRetrigger.OK {
		d.Errors = append(d.Errors, fmt.Sprintf("late retrigger mismatch: %+v", d.LateRetrigger))
	}
}

func runBattleBudgetCase(d *voiceBattleRuntimeDump) {
	a := litaudio.NewAllocator(litaudio.FalloffRadius)
	for i := 0; i < litaudio.WorldVoices; i++ {
		recordBattleAdmit(d, a, "all-partitions-budget", fmt.Sprintf("world-fill-%02d", i), litaudio.VoiceRequest{
			Cue: uint32(7000 + i), Asset: uint32(7000 + i), Partition: litaudio.PartitionWorld,
			Priority: litaudio.PrioAttackImpact, HasPos: true, Pos: litaudio.Vec3{X: 10}, Volume: 1, TimeMs: int64(i),
		})
	}
	for i := 0; i < litaudio.UIVoices; i++ {
		recordBattleAdmit(d, a, "all-partitions-budget", fmt.Sprintf("ui-fill-%02d", i), litaudio.VoiceRequest{
			Cue: uint32(7100 + i), Asset: uint32(7100 + i), Partition: litaudio.PartitionUI,
			Priority: litaudio.PrioAlert, Volume: 1, TimeMs: int64(100 + i),
		})
	}
	for i := 0; i < litaudio.StreamVoices; i++ {
		recordBattleAdmit(d, a, "all-partitions-budget", fmt.Sprintf("stream-fill-%02d", i), litaudio.VoiceRequest{
			Cue: uint32(7200 + i), Asset: uint32(7200 + i), Partition: litaudio.PartitionStream,
			Priority: litaudio.PrioAmbient, Volume: 1, TimeMs: int64(200 + i),
		})
	}
	extra := recordBattleAdmit(d, a, "all-partitions-budget", "ui-overflow-drops", litaudio.VoiceRequest{
		Cue: 7300, Asset: 7300, Partition: litaudio.PartitionUI, Priority: litaudio.PrioAlert, Volume: 1, TimeMs: 300,
	})
	d.Budget = voiceBudgetDump{
		World:        a.ActiveIn(litaudio.PartitionWorld),
		UI:           a.ActiveIn(litaudio.PartitionUI),
		Stream:       a.ActiveIn(litaudio.PartitionStream),
		Total:        a.ActiveTotal(),
		ExtraOutcome: extra.Outcome.String(),
		OK: a.ActiveIn(litaudio.PartitionWorld) == litaudio.WorldVoices &&
			a.ActiveIn(litaudio.PartitionUI) == litaudio.UIVoices &&
			a.ActiveIn(litaudio.PartitionStream) == litaudio.StreamVoices &&
			a.ActiveTotal() == litaudio.TotalVoices &&
			extra.Outcome == litaudio.Dropped,
	}
	if !d.Budget.OK {
		d.Errors = append(d.Errors, fmt.Sprintf("budget mismatch: %+v", d.Budget))
	}
}

func recordBattleAdmit(d *voiceBattleRuntimeDump, a *litaudio.Allocator, caseName, event string, req litaudio.VoiceRequest) litaudio.Decision {
	before := battleSlotSnapshot(a)
	dec := a.Admit(req)
	counts := voiceCountTraceDump{
		Event:  event,
		World:  a.ActiveIn(litaudio.PartitionWorld),
		UI:     a.ActiveIn(litaudio.PartitionUI),
		Stream: a.ActiveIn(litaudio.PartitionStream),
		Total:  a.ActiveTotal(),
	}
	d.VoiceCountTrace = append(d.VoiceCountTrace, counts)
	if counts.World > d.MaxWorldVoices {
		d.MaxWorldVoices = counts.World
	}
	if counts.UI > d.MaxUIVoices {
		d.MaxUIVoices = counts.UI
	}
	if counts.Stream > d.MaxStreamVoices {
		d.MaxStreamVoices = counts.Stream
	}
	if counts.Total > d.MaxTotalVoices {
		d.MaxTotalVoices = counts.Total
	}
	victim := voiceSlotDump{}
	if dec.Victim >= 0 && dec.Victim < len(before) {
		victim = before[dec.Victim]
	}
	d.Events = append(d.Events, voiceAdmissionEventDump{
		Case:           caseName,
		Event:          event,
		Rule:           admissionRule(dec.Outcome),
		Outcome:        dec.Outcome.String(),
		Cue:            req.Cue,
		Asset:          req.Asset,
		Partition:      partitionName(req.Partition),
		Priority:       priorityName(req.Priority),
		TimeMs:         req.TimeMs,
		Distance:       requestDistance(req),
		Slot:           dec.Slot,
		Victim:         dec.Victim,
		VictimCue:      victim.Cue,
		VictimPriority: victim.Priority,
		VictimDistance: victim.Distance,
		GainBump:       dec.GainBump,
		Counts:         counts,
	})
	return dec
}

func battleSlotSnapshot(a *litaudio.Allocator) []voiceSlotDump {
	out := make([]voiceSlotDump, 0, litaudio.TotalVoices)
	for i := 0; i < litaudio.TotalVoices; i++ {
		req, gain, active := a.Slot(i)
		slot := voiceSlotDump{Slot: i, Active: active}
		if active {
			slot.Cue = req.Cue
			slot.Asset = req.Asset
			slot.Partition = partitionName(req.Partition)
			slot.Priority = priorityName(req.Priority)
			slot.Distance = requestDistance(req)
			slot.Gain = gain
		}
		out = append(out, slot)
	}
	return out
}

func maxSlotGain(a *litaudio.Allocator) float64 {
	max := 0.0
	for i := 0; i < litaudio.TotalVoices; i++ {
		_, gain, active := a.Slot(i)
		if active && gain > max {
			max = gain
		}
	}
	return max
}

func battleAdmissionAllocsPerRun() float64 {
	a := litaudio.NewAllocator(litaudio.FalloffRadius)
	for i := 0; i < litaudio.WorldVoices; i++ {
		a.Admit(litaudio.VoiceRequest{
			Cue: uint32(8000 + i), Asset: uint32(8000 + i), Partition: litaudio.PartitionWorld,
			Priority: litaudio.PrioAmbient, HasPos: true, Pos: litaudio.Vec3{X: float64(i)}, Volume: 1, TimeMs: int64(i),
		})
	}
	req := litaudio.VoiceRequest{
		Cue: 9000, Asset: 9000, Partition: litaudio.PartitionWorld,
		Priority: litaudio.PrioAlert, HasPos: true, Pos: litaudio.Vec3{X: 5}, Volume: 1, TimeMs: 100,
	}
	return testing.AllocsPerRun(200, func() {
		_ = a.Admit(req)
	})
}

func admissionRule(o litaudio.Outcome) string {
	switch o {
	case litaudio.CulledDistance:
		return "distance-cull"
	case litaudio.Coalesced:
		return "duplicate-coalescing"
	case litaudio.Stolen:
		return "priority-eviction"
	case litaudio.Dropped:
		return "drop"
	case litaudio.Admitted:
		return "admit"
	default:
		return "unknown"
	}
}

func partitionName(p litaudio.Partition) string {
	switch p {
	case litaudio.PartitionWorld:
		return "world"
	case litaudio.PartitionUI:
		return "ui"
	case litaudio.PartitionStream:
		return "stream"
	default:
		return "unknown"
	}
}

func priorityName(p litaudio.Priority) string {
	switch p {
	case litaudio.PrioAmbient:
		return "ambient"
	case litaudio.PrioAttackImpact:
		return "attackimpact"
	case litaudio.PrioDeath:
		return "death"
	case litaudio.PrioAbilityCast:
		return "abilitycast"
	case litaudio.PrioAlert:
		return "alert"
	default:
		return "unknown"
	}
}

func requestDistance(req litaudio.VoiceRequest) float64 {
	if !req.HasPos {
		return 0
	}
	return math.Sqrt(req.Pos.X*req.Pos.X + req.Pos.Y*req.Pos.Y + req.Pos.Z*req.Pos.Z)
}
