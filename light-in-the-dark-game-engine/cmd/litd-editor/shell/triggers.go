package shell

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"regexp"
	"sort"
	"strconv"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

const (
	triggerGraphPath  = "data/editor/triggers.json"
	triggerScriptPath = "scripts/main.lua"
	triggerSchema     = 1
)

var triggerIDPattern = regexp.MustCompile(`^[a-z][a-z0-9_-]{0,63}$`)

type TriggerGraph struct {
	Schema     int               `json:"schema"`
	Categories []TriggerCategory `json:"categories,omitempty"`
	Triggers   []TriggerDraft    `json:"triggers"`
}

type TriggerCategory struct {
	ID   string `json:"id"`
	Name string `json:"name"`
}

type TriggerDraft struct {
	ID          string                 `json:"id"`
	Name        string                 `json:"name"`
	Category    string                 `json:"category,omitempty"`
	Comment     string                 `json:"comment,omitempty"`
	Enabled     bool                   `json:"enabled"`
	InitiallyOn bool                   `json:"initiallyOn"`
	Events      []TriggerEventDraft    `json:"events"`
	Condition   *TriggerConditionDraft `json:"condition,omitempty"`
	Actions     []TriggerActionDraft   `json:"actions"`
}

type TriggerEventKind string

const (
	TriggerEventUnitEntersRegion TriggerEventKind = "unit-enters-region"
	TriggerEventTimer            TriggerEventKind = "timer"
)

type TriggerEventDraft struct {
	Kind    TriggerEventKind `json:"kind"`
	Region  TriggerRegion    `json:"region,omitempty"`
	Seconds float64          `json:"seconds,omitempty"`
}

type TriggerRegion struct {
	MinX float64 `json:"minX"`
	MinY float64 `json:"minY"`
	MaxX float64 `json:"maxX"`
	MaxY float64 `json:"maxY"`
}

type TriggerConditionKind string

const (
	TriggerConditionAlways        TriggerConditionKind = "always"
	TriggerConditionOwnerIsPlayer TriggerConditionKind = "owner-is-player"
	TriggerConditionAnd           TriggerConditionKind = "and"
	TriggerConditionOr            TriggerConditionKind = "or"
	TriggerConditionNot           TriggerConditionKind = "not"
)

type TriggerConditionDraft struct {
	Kind     TriggerConditionKind    `json:"kind"`
	Player   int                     `json:"player,omitempty"`
	Children []TriggerConditionDraft `json:"children,omitempty"`
}

type TriggerActionKind string

const (
	TriggerActionKillEventUnit TriggerActionKind = "kill-event-unit"
	TriggerActionEnableTrigger TriggerActionKind = "enable-trigger"
	TriggerActionSetStorageInt TriggerActionKind = "set-storage-int"
)

type TriggerActionDraft struct {
	Kind      TriggerActionKind `json:"kind"`
	TargetID  string            `json:"targetId,omitempty"`
	Namespace string            `json:"namespace,omitempty"`
	Key       string            `json:"key,omitempty"`
	Value     int               `json:"value,omitempty"`
}

type TriggerEditorState struct {
	Graph       TriggerGraph
	LastOutputs []TriggerOutput
	LastError   string
}

type TriggerEditorSnapshot struct {
	Graph       TriggerGraph    `json:"graph"`
	Valid       bool            `json:"valid"`
	Errors      []string        `json:"errors,omitempty"`
	Summary     TriggerSummary  `json:"summary"`
	LastOutputs []TriggerOutput `json:"lastOutputs,omitempty"`
	LastError   string          `json:"lastError,omitempty"`
}

type TriggerSummary struct {
	Triggers int      `json:"triggers"`
	Events   int      `json:"events"`
	Actions  int      `json:"actions"`
	Outputs  []string `json:"outputs,omitempty"`
}

type TriggerOutput struct {
	Path   string `json:"path"`
	Bytes  int    `json:"bytes"`
	SHA256 string `json:"sha256,omitempty"`
}

type triggerBuild struct {
	graph   TriggerGraph
	script  []byte
	source  []byte
	outputs []TriggerOutput
}

func DefaultTriggerDraft(id, name string) TriggerDraft {
	return TriggerDraft{ID: id, Name: name, Enabled: true, InitiallyOn: true}
}

func (a *App) SaveTriggerGraph(graph TriggerGraph) (TriggerEditorSnapshot, error) {
	if a.world == nil {
		return TriggerEditorSnapshot{}, fmt.Errorf("editor triggers: no project loaded")
	}
	if a.archiveReadOnly {
		err := fmt.Errorf("editor triggers: archive opened read-only; source-form payload required for trigger authoring")
		a.errText = err.Error()
		a.status = a.errText
		return a.TriggerEditorSnapshot(), err
	}
	build, err := buildTriggerGraph(graph)
	if err != nil {
		a.triggers.Graph = normalizeTriggerGraphForDisplay(graph)
		a.triggers.LastOutputs = nil
		a.triggers.LastError = err.Error()
		a.errText = err.Error()
		a.status = a.errText
		a.mode = ModeTriggers
		return a.TriggerEditorSnapshot(), err
	}
	if err := a.world.SetPassthroughFile(triggerGraphPath, build.source); err != nil {
		a.triggers.LastError = err.Error()
		a.errText = err.Error()
		a.status = a.errText
		return a.TriggerEditorSnapshot(), err
	}
	if err := a.world.SetScript(triggerScriptPath, build.script); err != nil {
		a.triggers.LastError = err.Error()
		a.errText = err.Error()
		a.status = a.errText
		return a.TriggerEditorSnapshot(), err
	}
	a.triggers.Graph = build.graph
	a.triggers.LastOutputs = build.outputs
	a.triggers.LastError = ""
	a.errText = ""
	a.mode = ModeTriggers
	a.status = fmt.Sprintf("Triggers saved: %d", len(build.graph.Triggers))
	return a.TriggerEditorSnapshot(), nil
}

func (a *App) TriggerEditorSnapshot() TriggerEditorSnapshot {
	snap := TriggerEditorSnapshot{
		Graph:       normalizeTriggerGraphForDisplay(a.triggers.Graph),
		LastOutputs: append([]TriggerOutput(nil), a.triggers.LastOutputs...),
		LastError:   a.triggers.LastError,
	}
	build, err := buildTriggerGraph(snap.Graph)
	if err != nil {
		snap.Errors = strings.Split(err.Error(), "; ")
		return snap
	}
	snap.Graph = build.graph
	snap.Valid = true
	snap.Summary = triggerSummary(build.graph, build.outputs)
	return snap
}

func loadTriggerEditorState(w *sourceform.World) TriggerEditorState {
	var state TriggerEditorState
	graph, ok, err := loadTriggerGraph(w)
	if err != nil {
		state.LastError = err.Error()
		return state
	}
	if !ok {
		state.Graph = TriggerGraph{Schema: triggerSchema}
		return state
	}
	build, err := buildTriggerGraph(graph)
	if err != nil {
		state.Graph = normalizeTriggerGraphForDisplay(graph)
		state.LastError = err.Error()
		return state
	}
	state.Graph = build.graph
	state.LastOutputs = build.outputs
	return state
}

func loadTriggerGraph(w *sourceform.World) (TriggerGraph, bool, error) {
	body, ok, err := w.PassthroughFile(triggerGraphPath)
	if err != nil || !ok {
		return TriggerGraph{Schema: triggerSchema}, ok, err
	}
	var graph TriggerGraph
	dec := json.NewDecoder(strings.NewReader(string(body)))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&graph); err != nil {
		return TriggerGraph{}, true, fmt.Errorf("editor triggers: decode %s: %w", triggerGraphPath, err)
	}
	return graph, true, nil
}

func buildTriggerGraph(graph TriggerGraph) (triggerBuild, error) {
	graph = normalizeTriggerGraphForDisplay(graph)
	var errs []string
	if graph.Schema == 0 {
		graph.Schema = triggerSchema
	}
	if graph.Schema != triggerSchema {
		errs = append(errs, fmt.Sprintf("schema %d unsupported", graph.Schema))
	}
	categoryIDs := map[string]bool{}
	for i := range graph.Categories {
		cat := graph.Categories[i]
		if !triggerIDPattern.MatchString(cat.ID) {
			errs = append(errs, fmt.Sprintf("category %d id %q must match %s", i, cat.ID, triggerIDPattern.String()))
		}
		if categoryIDs[cat.ID] {
			errs = append(errs, fmt.Sprintf("category id %q duplicated", cat.ID))
		}
		categoryIDs[cat.ID] = true
	}
	triggerIDs := map[string]bool{}
	for i := range graph.Triggers {
		tr := graph.Triggers[i]
		if !triggerIDPattern.MatchString(tr.ID) {
			errs = append(errs, fmt.Sprintf("trigger %d id %q must match %s", i, tr.ID, triggerIDPattern.String()))
		}
		if triggerIDs[tr.ID] {
			errs = append(errs, fmt.Sprintf("trigger id %q duplicated", tr.ID))
		}
		triggerIDs[tr.ID] = true
		if tr.Name == "" {
			errs = append(errs, fmt.Sprintf("trigger %q name is required", tr.ID))
		}
		if tr.Category != "" && !categoryIDs[tr.Category] {
			errs = append(errs, fmt.Sprintf("trigger %q unknown category %q", tr.ID, tr.Category))
		}
		if len(tr.Events) == 0 {
			errs = append(errs, fmt.Sprintf("trigger %q needs at least one event", tr.ID))
		}
		if len(tr.Actions) == 0 {
			errs = append(errs, fmt.Sprintf("trigger %q needs at least one action", tr.ID))
		}
		for j, ev := range tr.Events {
			if err := validateTriggerEvent(ev); err != nil {
				errs = append(errs, fmt.Sprintf("trigger %q event %d: %v", tr.ID, j, err))
			}
		}
		if tr.Condition != nil {
			if err := validateTriggerCondition(*tr.Condition); err != nil {
				errs = append(errs, fmt.Sprintf("trigger %q condition: %v", tr.ID, err))
			}
		}
		for j, act := range tr.Actions {
			if err := validateTriggerAction(act); err != nil {
				errs = append(errs, fmt.Sprintf("trigger %q action %d: %v", tr.ID, j, err))
			}
		}
	}
	for _, tr := range graph.Triggers {
		for j, act := range tr.Actions {
			if act.Kind == TriggerActionEnableTrigger && !triggerIDs[act.TargetID] {
				errs = append(errs, fmt.Sprintf("trigger %q action %d target %q not found", tr.ID, j, act.TargetID))
			}
		}
	}
	if len(errs) > 0 {
		return triggerBuild{}, fmt.Errorf("editor triggers: %s", strings.Join(errs, "; "))
	}
	source, err := marshalTriggerGraph(graph)
	if err != nil {
		return triggerBuild{}, err
	}
	script, err := renderTriggerLua(graph)
	if err != nil {
		return triggerBuild{}, err
	}
	outputs := triggerOutputs([]triggerFile{
		{path: triggerGraphPath, body: source},
		{path: triggerScriptPath, body: script},
	})
	return triggerBuild{graph: graph, source: source, script: script, outputs: outputs}, nil
}

func validateTriggerEvent(ev TriggerEventDraft) error {
	switch ev.Kind {
	case TriggerEventUnitEntersRegion:
		if ev.Region.MaxX == ev.Region.MinX || ev.Region.MaxY == ev.Region.MinY {
			return fmt.Errorf("region rect must have non-zero width and height")
		}
	case TriggerEventTimer:
		if ev.Seconds <= 0 {
			return fmt.Errorf("timer seconds must be > 0")
		}
	default:
		return fmt.Errorf("unknown event kind %q", ev.Kind)
	}
	return nil
}

func validateTriggerCondition(c TriggerConditionDraft) error {
	switch c.Kind {
	case "", TriggerConditionAlways:
		return nil
	case TriggerConditionOwnerIsPlayer:
		if c.Player < 0 || c.Player > 15 {
			return fmt.Errorf("owner player %d outside 0..15", c.Player)
		}
	case TriggerConditionAnd, TriggerConditionOr:
		if len(c.Children) == 0 {
			return fmt.Errorf("%s condition needs at least one child", c.Kind)
		}
		for i, child := range c.Children {
			if err := validateTriggerCondition(child); err != nil {
				return fmt.Errorf("child %d: %w", i, err)
			}
		}
	case TriggerConditionNot:
		if len(c.Children) != 1 {
			return fmt.Errorf("not condition needs exactly one child")
		}
		return validateTriggerCondition(c.Children[0])
	default:
		return fmt.Errorf("unknown condition kind %q", c.Kind)
	}
	return nil
}

func validateTriggerAction(act TriggerActionDraft) error {
	switch act.Kind {
	case TriggerActionKillEventUnit:
		return nil
	case TriggerActionEnableTrigger:
		if strings.TrimSpace(act.TargetID) == "" {
			return fmt.Errorf("enable-trigger action needs targetId")
		}
	case TriggerActionSetStorageInt:
		if strings.TrimSpace(act.Namespace) == "" || strings.TrimSpace(act.Key) == "" {
			return fmt.Errorf("set-storage-int action needs namespace and key")
		}
	default:
		return fmt.Errorf("unknown action kind %q", act.Kind)
	}
	return nil
}

func renderTriggerLua(graph TriggerGraph) ([]byte, error) {
	var b strings.Builder
	b.WriteString("-- Generated by Light in the Dark editor trigger authoring.\n")
	b.WriteString("-- Source: data/editor/triggers.json\n")
	b.WriteString("LitD_Editor_Triggers = LitD_Editor_Triggers or {}\n\n")
	for _, tr := range graph.Triggers {
		name := triggerLuaName(tr.ID)
		fmt.Fprintf(&b, "-- %s\n", tr.Name)
		if tr.Comment != "" {
			for _, line := range strings.Split(tr.Comment, "\n") {
				fmt.Fprintf(&b, "-- %s\n", line)
			}
		}
		fmt.Fprintf(&b, "local %s = CreateTrigger()\n", name)
		fmt.Fprintf(&b, "LitD_Editor_Triggers[%s] = %s\n", strconv.Quote(tr.ID), name)
	}
	if len(graph.Triggers) > 0 {
		b.WriteByte('\n')
	}
	for _, tr := range graph.Triggers {
		name := triggerLuaName(tr.ID)
		for i, ev := range tr.Events {
			switch ev.Kind {
			case TriggerEventUnitEntersRegion:
				region := fmt.Sprintf("%s_region_%d", name, i+1)
				fmt.Fprintf(&b, "local %s = Game_NewRegion()\n", region)
				fmt.Fprintf(&b, "Region_AddRect(%s, {minx = %s, miny = %s, maxx = %s, maxy = %s})\n",
					region, luaFloat(ev.Region.MinX), luaFloat(ev.Region.MinY), luaFloat(ev.Region.MaxX), luaFloat(ev.Region.MaxY))
				fmt.Fprintf(&b, "TriggerRegisterEnterRegion(%s, %s)\n", name, region)
			case TriggerEventTimer:
				fmt.Fprintf(&b, "TriggerRegisterTimerEvent(%s, %s)\n", name, luaFloat(ev.Seconds))
			default:
				return nil, fmt.Errorf("editor triggers: unknown event kind %q", ev.Kind)
			}
		}
		condExpr, err := renderConditionExpr(tr.Condition)
		if err != nil {
			return nil, err
		}
		condName := name + "_condition"
		fmt.Fprintf(&b, "function %s(e)\n\treturn %s\nend\n", condName, condExpr)
		fmt.Fprintf(&b, "TriggerAddCondition(%s, %s)\n", name, condName)
		actionName := name + "_action"
		fmt.Fprintf(&b, "function %s(e)\n", actionName)
		for _, act := range tr.Actions {
			if err := renderAction(&b, act); err != nil {
				return nil, err
			}
		}
		b.WriteString("end\n")
		fmt.Fprintf(&b, "TriggerAddAction(%s, %s)\n", name, actionName)
		if !tr.Enabled || !tr.InitiallyOn {
			fmt.Fprintf(&b, "DisableTrigger(%s)\n", name)
		}
		b.WriteByte('\n')
	}
	return []byte(b.String()), nil
}

func renderConditionExpr(cond *TriggerConditionDraft) (string, error) {
	if cond == nil {
		return "true", nil
	}
	return renderConditionValue(*cond)
}

func renderConditionValue(cond TriggerConditionDraft) (string, error) {
	switch cond.Kind {
	case "", TriggerConditionAlways:
		return "true", nil
	case TriggerConditionOwnerIsPlayer:
		return fmt.Sprintf("Player_Slot(Unit_Owner(Event_Unit(e))) == %d", cond.Player), nil
	case TriggerConditionAnd, TriggerConditionOr:
		op := " and "
		if cond.Kind == TriggerConditionOr {
			op = " or "
		}
		parts := make([]string, 0, len(cond.Children))
		for _, child := range cond.Children {
			part, err := renderConditionValue(child)
			if err != nil {
				return "", err
			}
			parts = append(parts, part)
		}
		return "(" + strings.Join(parts, op) + ")", nil
	case TriggerConditionNot:
		child, err := renderConditionValue(cond.Children[0])
		if err != nil {
			return "", err
		}
		return "not (" + child + ")", nil
	default:
		return "", fmt.Errorf("editor triggers: unknown condition kind %q", cond.Kind)
	}
}

func renderAction(b *strings.Builder, act TriggerActionDraft) error {
	switch act.Kind {
	case TriggerActionKillEventUnit:
		b.WriteString("\tUnit_Kill(Event_Unit(e))\n")
	case TriggerActionEnableTrigger:
		fmt.Fprintf(b, "\tEnableTrigger(%s)\n", triggerLuaName(act.TargetID))
	case TriggerActionSetStorageInt:
		b.WriteString("\tlocal litd_store = Game_Storage()\n")
		fmt.Fprintf(b, "\tStorage_SetInt(litd_store, %s, %s, %d)\n", strconv.Quote(act.Namespace), strconv.Quote(act.Key), act.Value)
	default:
		return fmt.Errorf("editor triggers: unknown action kind %q", act.Kind)
	}
	return nil
}

func normalizeTriggerGraphForDisplay(graph TriggerGraph) TriggerGraph {
	if graph.Schema == 0 {
		graph.Schema = triggerSchema
	}
	for i := range graph.Categories {
		graph.Categories[i].ID = normalizeTriggerID(graph.Categories[i].ID)
		graph.Categories[i].Name = strings.TrimSpace(graph.Categories[i].Name)
	}
	for i := range graph.Triggers {
		tr := &graph.Triggers[i]
		tr.ID = normalizeTriggerID(tr.ID)
		tr.Name = strings.TrimSpace(tr.Name)
		tr.Category = normalizeTriggerID(tr.Category)
		tr.Comment = strings.TrimSpace(tr.Comment)
		for j := range tr.Actions {
			tr.Actions[j].TargetID = normalizeTriggerID(tr.Actions[j].TargetID)
			tr.Actions[j].Namespace = strings.TrimSpace(tr.Actions[j].Namespace)
			tr.Actions[j].Key = strings.TrimSpace(tr.Actions[j].Key)
		}
	}
	return graph
}

func normalizeTriggerID(id string) string {
	return strings.ToLower(strings.TrimSpace(id))
}

func marshalTriggerGraph(graph TriggerGraph) ([]byte, error) {
	body, err := json.MarshalIndent(graph, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("editor triggers: encode graph: %w", err)
	}
	return append(body, '\n'), nil
}

func triggerLuaName(id string) string {
	var b strings.Builder
	b.WriteString("litd_trigger_")
	for _, r := range id {
		switch {
		case r >= 'a' && r <= 'z':
			b.WriteRune(r)
		case r >= '0' && r <= '9':
			b.WriteRune(r)
		default:
			b.WriteByte('_')
		}
	}
	return b.String()
}

func luaFloat(v float64) string {
	return strconv.FormatFloat(v, 'f', -1, 64)
}

type triggerFile struct {
	path string
	body []byte
}

func triggerOutputs(files []triggerFile) []TriggerOutput {
	out := make([]TriggerOutput, 0, len(files))
	for _, file := range files {
		sum := sha256.Sum256(file.body)
		out = append(out, TriggerOutput{Path: file.path, Bytes: len(file.body), SHA256: hex.EncodeToString(sum[:])})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Path < out[j].Path })
	return out
}

func triggerSummary(graph TriggerGraph, outputs []TriggerOutput) TriggerSummary {
	var events, actions int
	for _, tr := range graph.Triggers {
		events += len(tr.Events)
		actions += len(tr.Actions)
	}
	paths := make([]string, len(outputs))
	for i, out := range outputs {
		paths[i] = out.Path
	}
	return TriggerSummary{Triggers: len(graph.Triggers), Events: events, Actions: actions, Outputs: paths}
}
