package main

import (
	"fmt"
	"math"
	"strings"
	"testing/fstest"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	litaudio "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
)

type audioDomainRuntimeDump struct {
	OK                  bool                       `json:"ok"`
	Scene               string                     `json:"scene"`
	SourceOfTruth       string                     `json:"sourceOfTruth"`
	ReferenceDistance   float64                    `json:"referenceDistance"`
	MaxAudibleDistance  float64                    `json:"maxAudibleDistance"`
	PanWidth            float64                    `json:"panWidth"`
	CameraFocus         litaudio.Vec3              `json:"cameraFocus"`
	SoundTable          []audioDomainSoundRowDump  `json:"soundTable"`
	Playbacks           []audioDomainPlaybackDump  `json:"playbacks"`
	DistanceTrace       []audioDomainPlaybackDump  `json:"distanceTrace"`
	PanTrace            []audioDomainPlaybackDump  `json:"panTrace"`
	VolumeGroup         audioDomainVolumeGroupDump `json:"volumeGroup"`
	AssetcheckRejection audioDomainAssetcheckDump  `json:"assetcheckRejection"`
	Snapshot            litaudio.Snapshot          `json:"snapshot"`
	AudibleDescription  string                     `json:"audibleDescription"`
	Errors              []string                   `json:"errors,omitempty"`
}

type audioDomainSoundRowDump struct {
	Cue      string `json:"cue"`
	Domain   string `json:"domain"`
	Group    string `json:"group"`
	Priority string `json:"priority"`
	Ogg      string `json:"ogg"`
}

type audioDomainPlaybackDump struct {
	Label            string          `json:"label"`
	Rule             string          `json:"rule"`
	CueName          string          `json:"cueName"`
	Cue              uint32          `json:"cue"`
	Channel          string          `json:"channel"`
	Domain           string          `json:"domain"`
	Group            string          `json:"group"`
	Position         litaudio.Vec3   `json:"position"`
	HasPosition      bool            `json:"hasPosition"`
	Distance         float64         `json:"distance"`
	RequestedVolume  float64         `json:"requestedVolume"`
	Outcome          string          `json:"outcome"`
	Gain             float64         `json:"gain"`
	Pan              float64         `json:"pan"`
	Culled           bool            `json:"culled"`
	Slot             int             `json:"slot"`
	VoiceCountBefore int             `json:"voiceCountBefore"`
	VoiceCountAfter  int             `json:"voiceCountAfter"`
	CulledBefore     int             `json:"culledBefore"`
	CulledAfter      int             `json:"culledAfter"`
	Voice            *litaudio.Voice `json:"voice,omitempty"`
}

type audioDomainVolumeGroupDump struct {
	WorldCue        string            `json:"worldCue"`
	UICue           string            `json:"uiCue"`
	Before          litaudio.Snapshot `json:"before"`
	After           litaudio.Snapshot `json:"after"`
	WorldGainBefore float64           `json:"worldGainBefore"`
	WorldGainAfter  float64           `json:"worldGainAfter"`
	UIGainBefore    float64           `json:"uiGainBefore"`
	UIGainAfter     float64           `json:"uiGainAfter"`
	OK              bool              `json:"ok"`
}

type audioDomainAssetcheckDump struct {
	Path   string `json:"path"`
	Rule   string `json:"rule"`
	Output string `json:"output"`
	OK     bool   `json:"ok"`
}

type audioDomainPlaybackSpec struct {
	label   string
	rule    string
	cue     string
	kind    api.AudioEventKind
	channel api.SoundChannel
	pos     litaudio.Vec3
	hasPos  bool
	volume  float64
}

func buildAudioDomainDump(scene string) (*audioDomainRuntimeDump, error) {
	scene = strings.ToLower(strings.TrimSpace(scene))
	dump := &audioDomainRuntimeDump{
		Scene:              scene,
		SourceOfTruth:      "litd/audio.Manager.Dump() after scripted api.AudioEvent playback",
		ReferenceDistance:  litaudio.ReferenceDistance,
		MaxAudibleDistance: litaudio.MaxAudibleDistance,
		PanWidth:           litaudio.PanWidth,
		CameraFocus:        litaudio.Vec3{},
		AudibleDescription: "Expected on hardware: world emitters pan left/right and fade with distance until the 3-screen world event is silent; UI click and stinger remain centered at full gain even when played with far world context.",
	}
	if scene != "basecamp" {
		err := fmt.Errorf("audio domain fixture requires -scene basecamp, got %q", scene)
		dump.Errors = append(dump.Errors, err.Error())
		return dump, err
	}

	table, err := loadAudioDomainSoundTable()
	if err != nil {
		dump.Errors = append(dump.Errors, err.Error())
		return dump, err
	}
	dump.SoundTable = audioDomainSoundRows(table)

	m := litaudio.NewManager(nil)
	defer m.Close()
	m.SetSoundTable(table)
	m.SetListener(dump.CameraFocus)

	specs := []audioDomainPlaybackSpec{
		{
			label: "world-camera-center", rule: "world at camera focus is full gain and centered",
			cue: "renderdemo/domains/world-center", kind: api.AudioPlayAt, channel: api.ChannelEffects,
			pos: litaudio.Vec3{}, hasPos: true, volume: 1,
		},
		{
			label: "world-viewport-edge", rule: "world at reference distance remains full gain",
			cue: "renderdemo/domains/world-edge", kind: api.AudioPlayAt, channel: api.ChannelEffects,
			pos: litaudio.Vec3{X: litaudio.ReferenceDistance}, hasPos: true, volume: 1,
		},
		{
			label: "world-one-point-five-screens", rule: "world at max audible radius is quiet but still admitted",
			cue: "renderdemo/domains/world-one-point-five", kind: api.AudioPlayAt, channel: api.ChannelEffects,
			pos: litaudio.Vec3{X: litaudio.MaxAudibleDistance}, hasPos: true, volume: 1,
		},
		{
			label: "world-three-screens", rule: "world beyond max audible radius is distance-culled",
			cue: "renderdemo/domains/world-three-screens", kind: api.AudioPlayAt, channel: api.ChannelEffects,
			pos: litaudio.Vec3{X: litaudio.MaxAudibleDistance * 2}, hasPos: true, volume: 1,
		},
		{
			label: "ui-click", rule: "UI click is flat full gain with no position",
			cue: "renderdemo/domains/ui-click", kind: api.AudioPlay, channel: api.ChannelUI,
			volume: 1,
		},
		{
			label: "ui-stinger-camera-far", rule: "UI table classification beats far world position and Effects channel",
			cue: "renderdemo/domains/ui-stinger", kind: api.AudioPlayAt, channel: api.ChannelEffects,
			pos: litaudio.Vec3{X: litaudio.MaxAudibleDistance * 2}, hasPos: true, volume: 1,
		},
		{
			label: "world-hard-left", rule: "hard-left world emitter pans negative",
			cue: "renderdemo/domains/world-hard-left", kind: api.AudioPlayAt, channel: api.ChannelEffects,
			pos: litaudio.Vec3{X: -litaudio.PanWidth}, hasPos: true, volume: 1,
		},
		{
			label: "world-hard-right", rule: "hard-right world emitter pans positive",
			cue: "renderdemo/domains/world-hard-right", kind: api.AudioPlayAt, channel: api.ChannelEffects,
			pos: litaudio.Vec3{X: litaudio.PanWidth}, hasPos: true, volume: 1,
		},
	}
	for _, spec := range specs {
		p := recordAudioDomainPlayback(m, table, spec)
		dump.Playbacks = append(dump.Playbacks, p)
		switch p.Label {
		case "world-camera-center", "world-viewport-edge", "world-one-point-five-screens", "world-three-screens":
			dump.DistanceTrace = append(dump.DistanceTrace, p)
		case "world-hard-left", "world-hard-right":
			dump.PanTrace = append(dump.PanTrace, p)
		}
	}
	dump.Snapshot = m.Dump()
	dump.VolumeGroup = buildAudioDomainVolumeGroup(table)
	dump.AssetcheckRejection = buildAudioDomainAssetcheckRejection()
	validateAudioDomainDump(dump)
	return dump, nil
}

func loadAudioDomainSoundTable() (*litaudio.SoundTable, error) {
	const body = `
[[sound]]
cue = "renderdemo/domains/world-center"
domain = "world"
priority = "attackimpact"
ogg = "sfx/world_center.ogg"

[[sound]]
cue = "renderdemo/domains/world-edge"
domain = "world"
priority = "attackimpact"
ogg = "sfx/world_edge.ogg"

[[sound]]
cue = "renderdemo/domains/world-one-point-five"
domain = "world"
priority = "attackimpact"
ogg = "sfx/world_one_point_five.ogg"

[[sound]]
cue = "renderdemo/domains/world-three-screens"
domain = "world"
priority = "attackimpact"
ogg = "sfx/world_three_screens.ogg"

[[sound]]
cue = "renderdemo/domains/world-hard-left"
domain = "world"
priority = "attackimpact"
ogg = "sfx/world_hard_left.ogg"

[[sound]]
cue = "renderdemo/domains/world-hard-right"
domain = "world"
priority = "attackimpact"
ogg = "sfx/world_hard_right.ogg"

[[sound]]
cue = "renderdemo/domains/world-volume"
domain = "world"
priority = "attackimpact"
ogg = "sfx/world_volume.ogg"

[[sound]]
cue = "renderdemo/domains/ui-click"
domain = "ui"
priority = "ambient"
ogg = "ui/click.ogg"

[[sound]]
cue = "renderdemo/domains/ui-stinger"
domain = "ui"
priority = "alert"
ogg = "ui/stinger.ogg"

[[sound]]
cue = "renderdemo/domains/ui-volume"
domain = "ui"
priority = "ambient"
ogg = "ui/volume.ogg"
`
	return litaudio.LoadSoundTable(fstest.MapFS{"audio/sounds.toml": {Data: []byte(body)}}, "audio/sounds.toml")
}

func audioDomainSoundRows(table *litaudio.SoundTable) []audioDomainSoundRowDump {
	rows := make([]audioDomainSoundRowDump, 0, table.Len())
	for _, cue := range table.Cues() {
		e, _ := table.Lookup(cue)
		rows = append(rows, audioDomainSoundRowDump{
			Cue:      e.Cue,
			Domain:   audioDomainString(e.Domain),
			Group:    audioGroupString(litaudio.GroupForDomain(e.Domain)),
			Priority: audioPriorityString(e.Priority),
			Ogg:      e.Ogg,
		})
	}
	return rows
}

func recordAudioDomainPlayback(m *litaudio.Manager, table *litaudio.SoundTable, spec audioDomainPlaybackSpec) audioDomainPlaybackDump {
	before := m.Dump()
	cue := api.CueID(spec.cue)
	ev := api.AudioEvent{
		Kind: spec.kind, Cue: cue, Volume: spec.volume, Pitch: 1,
		Channel: spec.channel, HasPos: spec.hasPos,
		Pos: api.Vec2{X: spec.pos.X, Y: spec.pos.Y}, Z: spec.pos.Z,
	}
	m.Handle(ev)
	after := m.Dump()

	entry, ok := table.Lookup(spec.cue)
	domain, group := "unclassified", "unclassified"
	if ok {
		domain = audioDomainString(entry.Domain)
		group = audioGroupString(litaudio.GroupForDomain(entry.Domain))
	}
	out := audioDomainPlaybackDump{
		Label:            spec.label,
		Rule:             spec.rule,
		CueName:          spec.cue,
		Cue:              cue,
		Channel:          audioChannelString(spec.channel),
		Domain:           domain,
		Group:            group,
		Position:         spec.pos,
		HasPosition:      spec.hasPos,
		Distance:         audioDomainDistance(spec.pos, after.Listener, spec.hasPos),
		RequestedVolume:  spec.volume,
		Outcome:          "missing",
		Slot:             -1,
		VoiceCountBefore: before.VoiceCount,
		VoiceCountAfter:  after.VoiceCount,
		CulledBefore:     before.Culled,
		CulledAfter:      after.Culled,
	}
	if v, ok := audioDomainVoiceByCue(after, cue); ok {
		vcopy := v
		out.Voice = &vcopy
		out.Outcome = "admitted"
		out.Domain = audioDomainString(v.Domain)
		out.Group = audioGroupString(v.Group)
		out.Gain = v.Gain
		out.Pan = v.Pan
		out.Culled = false
		out.Slot = v.Slot
		return out
	}
	if after.Culled > before.Culled {
		out.Outcome = litaudio.CulledDistance.String()
		out.Culled = true
	}
	return out
}

func buildAudioDomainVolumeGroup(table *litaudio.SoundTable) audioDomainVolumeGroupDump {
	const (
		worldCue = "renderdemo/domains/world-volume"
		uiCue    = "renderdemo/domains/ui-volume"
	)
	m := litaudio.NewManager(nil)
	defer m.Close()
	m.SetSoundTable(table)
	m.SetListener(litaudio.Vec3{})
	m.Handle(api.AudioEvent{
		Kind: api.AudioPlayAt, Cue: api.CueID(worldCue), Volume: 1, Pitch: 1,
		HasPos: true, Pos: api.Vec2{X: litaudio.ReferenceDistance, Y: 0}, Channel: api.ChannelEffects,
	})
	m.Handle(api.AudioEvent{
		Kind: api.AudioPlayAt, Cue: api.CueID(uiCue), Volume: 1, Pitch: 1,
		HasPos: true, Pos: api.Vec2{X: litaudio.MaxAudibleDistance * 2, Y: 0}, Channel: api.ChannelEffects,
	})
	before := m.Dump()
	m.SetGroupVolume(litaudio.GroupWorld, 0)
	after := m.Dump()
	worldBefore, _ := audioDomainVoiceByCue(before, api.CueID(worldCue))
	worldAfter, _ := audioDomainVoiceByCue(after, api.CueID(worldCue))
	uiBefore, _ := audioDomainVoiceByCue(before, api.CueID(uiCue))
	uiAfter, _ := audioDomainVoiceByCue(after, api.CueID(uiCue))
	out := audioDomainVolumeGroupDump{
		WorldCue:        worldCue,
		UICue:           uiCue,
		Before:          before,
		After:           after,
		WorldGainBefore: worldBefore.Gain,
		WorldGainAfter:  worldAfter.Gain,
		UIGainBefore:    uiBefore.Gain,
		UIGainAfter:     uiAfter.Gain,
	}
	out.OK = audioDomainApprox(out.WorldGainBefore, 1) &&
		audioDomainApprox(out.WorldGainAfter, 0) &&
		audioDomainApprox(out.UIGainBefore, 1) &&
		audioDomainApprox(out.UIGainAfter, 1)
	return out
}

func buildAudioDomainAssetcheckRejection() audioDomainAssetcheckDump {
	const body = `[[sound]]
cue = "renderdemo/domains/unclassified"
priority = "death"
ogg = "sfx/unclassified.ogg"
`
	_, err := litaudio.LoadSoundTable(fstest.MapFS{"audio/sounds.toml": {Data: []byte(body)}}, "audio/sounds.toml")
	output := ""
	if err != nil {
		output = "audio/sounds.toml: SOUND-CLASS: " + err.Error()
	}
	return audioDomainAssetcheckDump{
		Path:   "audio/sounds.toml",
		Rule:   "SOUND-CLASS",
		Output: output,
		OK: strings.Contains(output, `sound "renderdemo/domains/unclassified" has missing/invalid domain ""`) &&
			strings.Contains(output, "SOUND-CLASS"),
	}
}

func validateAudioDomainDump(d *audioDomainRuntimeDump) {
	byLabel := make(map[string]audioDomainPlaybackDump, len(d.Playbacks))
	for _, p := range d.Playbacks {
		byLabel[p.Label] = p
	}
	require := func(ok bool, msg string) {
		if !ok {
			d.Errors = append(d.Errors, msg)
		}
	}
	center := byLabel["world-camera-center"]
	edge := byLabel["world-viewport-edge"]
	oneHalf := byLabel["world-one-point-five-screens"]
	far := byLabel["world-three-screens"]
	uiClick := byLabel["ui-click"]
	uiStinger := byLabel["ui-stinger-camera-far"]
	left := byLabel["world-hard-left"]
	right := byLabel["world-hard-right"]

	require(center.Outcome == "admitted" && audioDomainApprox(center.Gain, 1) && audioDomainApprox(center.Pan, 0), "world center was not full-gain centered")
	require(edge.Outcome == "admitted" && audioDomainApprox(edge.Gain, 1), "world edge was not full-gain")
	require(oneHalf.Outcome == "admitted" && oneHalf.Gain < edge.Gain && audioDomainApprox(oneHalf.Gain, litaudio.ReferenceDistance/litaudio.MaxAudibleDistance), "world max-audible gain did not attenuate to reference/max")
	require(far.Outcome == litaudio.CulledDistance.String() && far.Culled && d.Snapshot.Culled == 1, "world three-screen emitter was not distance-culled")
	require(uiClick.Outcome == "admitted" && uiClick.Domain == "ui" && audioDomainApprox(uiClick.Gain, 1) && audioDomainApprox(uiClick.Pan, 0), "UI click was not flat full-gain")
	require(uiStinger.Outcome == "admitted" && uiStinger.Domain == "ui" && audioDomainApprox(uiStinger.Gain, 1) && audioDomainApprox(uiStinger.Pan, 0), "far UI stinger was not admitted flat/full")
	require(left.Outcome == "admitted" && left.Pan < 0 && right.Outcome == "admitted" && right.Pan > 0, "hard-left/right pan signs did not flip")
	require(d.VolumeGroup.OK, "World/UI volume group independence failed")
	require(d.AssetcheckRejection.OK, "SOUND-CLASS rejection output missing")
	d.OK = len(d.Errors) == 0
}

func audioDomainVoiceByCue(s litaudio.Snapshot, cue uint32) (litaudio.Voice, bool) {
	for _, v := range s.Voices {
		if v.Cue == cue {
			return v, true
		}
	}
	return litaudio.Voice{}, false
}

func audioDomainDistance(src, listener litaudio.Vec3, hasPos bool) float64 {
	if !hasPos {
		return 0
	}
	dx, dy, dz := src.X-listener.X, src.Y-listener.Y, src.Z-listener.Z
	return math.Sqrt(dx*dx + dy*dy + dz*dz)
}

func audioDomainApprox(a, b float64) bool { return math.Abs(a-b) < 1e-9 }

func audioDomainString(d litaudio.Domain) string {
	switch d {
	case litaudio.DomainWorld:
		return "world"
	case litaudio.DomainUI:
		return "ui"
	default:
		return "unknown"
	}
}

func audioGroupString(g litaudio.VolumeGroup) string {
	switch g {
	case litaudio.GroupWorld:
		return "world"
	case litaudio.GroupUI:
		return "ui"
	case litaudio.GroupMusic:
		return "music"
	default:
		return "unknown"
	}
}

func audioPriorityString(p litaudio.Priority) string {
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

func audioChannelString(ch api.SoundChannel) string {
	switch ch {
	case api.ChannelEffects:
		return "effects"
	case api.ChannelMusic:
		return "music"
	case api.ChannelAmbient:
		return "ambient"
	case api.ChannelUI:
		return "ui"
	case api.ChannelVoice:
		return "voice"
	default:
		return "unknown"
	}
}
