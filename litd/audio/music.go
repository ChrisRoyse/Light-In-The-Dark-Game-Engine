package audio

import (
	"fmt"
	"io/fs"
	"math/rand"
	"path"
	"sort"
	"strings"
	"time"

	"github.com/BurntSushi/toml"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio/oggmeta"
)

const (
	// MusicStreamSlot and AmbienceStreamSlot are the two reserved stream voices
	// in the fixed 32-voice allocator budget.
	MusicStreamSlot    = WorldVoices + UIVoices
	AmbienceStreamSlot = MusicStreamSlot + 1

	// MaxCrossfadeMs bounds data-table fades to values the stream controller can
	// audit in a single integer field without hiding bad content behind defaults.
	MaxCrossfadeMs = 30_000
)

// MusicPlaylist is the track list for one faction on a map.
type MusicPlaylist struct {
	Faction string
	Tracks  []string
}

// MusicMap is one map's ambience and faction playlist definition.
type MusicMap struct {
	ID           string
	Ambience     string
	AmbienceLoop bool
	CrossfadeMs  int
	Shuffle      bool
	playlists    map[string]MusicPlaylist
	order        []string
}

// Playlist returns a defensive copy of the faction playlist.
func (m MusicMap) Playlist(faction string) (MusicPlaylist, bool) {
	p, ok := m.playlists[strings.TrimSpace(faction)]
	if !ok {
		return MusicPlaylist{}, false
	}
	out := MusicPlaylist{Faction: p.Faction, Tracks: append([]string(nil), p.Tracks...)}
	return out, true
}

// Factions returns the map's playlist factions in deterministic order.
func (m MusicMap) Factions() []string {
	out := make([]string, len(m.order))
	copy(out, m.order)
	return out
}

// MusicSelection is the fully resolved map+faction stream recipe.
type MusicSelection struct {
	MapID        string
	Faction      string
	Ambience     string
	AmbienceLoop bool
	CrossfadeMs  int
	Shuffle      bool
	Tracks       []string
}

// MusicTable is the validated data/music table.
type MusicTable struct {
	byMap map[string]MusicMap
	order []string
}

// Len reports the number of map rows.
func (t *MusicTable) Len() int { return len(t.order) }

// Maps returns map ids in deterministic order.
func (t *MusicTable) Maps() []string {
	out := make([]string, len(t.order))
	copy(out, t.order)
	return out
}

// Lookup resolves mapID+faction to a stream selection. Returned slices are
// defensive copies so the stream controller can shuffle without mutating the
// table source of truth.
func (t *MusicTable) Lookup(mapID, faction string) (MusicSelection, bool) {
	if t == nil {
		return MusicSelection{}, false
	}
	m, ok := t.byMap[strings.TrimSpace(mapID)]
	if !ok {
		return MusicSelection{}, false
	}
	pl, ok := m.Playlist(faction)
	if !ok {
		return MusicSelection{}, false
	}
	return MusicSelection{
		MapID:        m.ID,
		Faction:      pl.Faction,
		Ambience:     m.Ambience,
		AmbienceLoop: m.AmbienceLoop,
		CrossfadeMs:  m.CrossfadeMs,
		Shuffle:      m.Shuffle,
		Tracks:       append([]string(nil), pl.Tracks...),
	}, true
}

type rawMusicTable struct {
	Map []rawMusicMap `toml:"map"`
}

type rawMusicMap struct {
	ID           string             `toml:"id"`
	Ambience     string             `toml:"ambience"`
	AmbienceLoop *bool              `toml:"ambience-loop"`
	CrossfadeMs  int                `toml:"crossfade-ms"`
	Shuffle      *bool              `toml:"shuffle"`
	Playlist     []rawMusicPlaylist `toml:"playlist"`
}

type rawMusicPlaylist struct {
	Faction string   `toml:"faction"`
	Tracks  []string `toml:"tracks"`
}

// LoadMusicTable parses and fail-closed validates a map/faction music table.
func LoadMusicTable(fsys fs.FS, p string) (*MusicTable, error) {
	body, err := fs.ReadFile(fsys, p)
	if err != nil {
		return nil, fmt.Errorf("music table %s: %w", p, err)
	}
	var raw rawMusicTable
	md, err := toml.Decode(string(body), &raw)
	if err != nil {
		return nil, fmt.Errorf("music table %s: %w", p, err)
	}
	for _, un := range md.Undecoded() {
		return nil, fmt.Errorf("music table %s: unknown field %q", p, un.String())
	}
	if len(raw.Map) == 0 {
		return nil, fmt.Errorf("music table %s: at least one [[map]] entry is required", p)
	}

	t := &MusicTable{byMap: make(map[string]MusicMap, len(raw.Map))}
	for i, rm := range raw.Map {
		id := strings.TrimSpace(rm.ID)
		if id == "" {
			return nil, fmt.Errorf("music table %s: map row %d has empty id", p, i)
		}
		if _, dup := t.byMap[id]; dup {
			return nil, fmt.Errorf("music table %s: duplicate map id %q", p, id)
		}
		amb, err := cleanMusicAssetPath(rm.Ambience)
		if err != nil {
			return nil, fmt.Errorf("music table %s: map %q ambience: %w", p, id, err)
		}
		if rm.AmbienceLoop == nil {
			return nil, fmt.Errorf("music table %s: map %q must declare ambience-loop", p, id)
		}
		if !*rm.AmbienceLoop {
			return nil, fmt.Errorf("music table %s: map %q ambience-loop must be true (ambience streams loop in-game)", p, id)
		}
		if rm.Shuffle == nil {
			return nil, fmt.Errorf("music table %s: map %q must declare shuffle", p, id)
		}
		if rm.CrossfadeMs < 0 || rm.CrossfadeMs > MaxCrossfadeMs {
			return nil, fmt.Errorf("music table %s: map %q crossfade-ms=%d outside [0,%d]", p, id, rm.CrossfadeMs, MaxCrossfadeMs)
		}
		if len(rm.Playlist) == 0 {
			return nil, fmt.Errorf("music table %s: map %q needs at least one [[map.playlist]]", p, id)
		}
		m := MusicMap{
			ID:           id,
			Ambience:     amb,
			AmbienceLoop: *rm.AmbienceLoop,
			CrossfadeMs:  rm.CrossfadeMs,
			Shuffle:      *rm.Shuffle,
			playlists:    make(map[string]MusicPlaylist, len(rm.Playlist)),
		}
		for j, rp := range rm.Playlist {
			faction := strings.TrimSpace(rp.Faction)
			if faction == "" {
				return nil, fmt.Errorf("music table %s: map %q playlist row %d has empty faction", p, id, j)
			}
			if _, dup := m.playlists[faction]; dup {
				return nil, fmt.Errorf("music table %s: map %q duplicate faction %q", p, id, faction)
			}
			if len(rp.Tracks) == 0 {
				return nil, fmt.Errorf("music table %s: map %q faction %q needs at least one track", p, id, faction)
			}
			tracks := make([]string, len(rp.Tracks))
			seen := map[string]bool{}
			for k, track := range rp.Tracks {
				clean, err := cleanMusicAssetPath(track)
				if err != nil {
					return nil, fmt.Errorf("music table %s: map %q faction %q track %d: %w", p, id, faction, k, err)
				}
				if seen[clean] {
					return nil, fmt.Errorf("music table %s: map %q faction %q duplicate track %q", p, id, faction, clean)
				}
				seen[clean] = true
				tracks[k] = clean
			}
			m.playlists[faction] = MusicPlaylist{Faction: faction, Tracks: tracks}
			m.order = append(m.order, faction)
		}
		sort.Strings(m.order)
		t.byMap[id] = m
		t.order = append(t.order, id)
	}
	sort.Strings(t.order)
	return t, nil
}

func cleanMusicAssetPath(p string) (string, error) {
	p = strings.TrimSpace(p)
	if p == "" {
		return "", fmt.Errorf("empty path")
	}
	if strings.Contains(p, "\\") {
		return "", fmt.Errorf("%q must use slash-separated paths", p)
	}
	clean := path.Clean(p)
	if clean != p {
		return "", fmt.Errorf("%q is not canonical (want %q)", p, clean)
	}
	if strings.HasPrefix(clean, "/") || clean == "." || clean == ".." || strings.HasPrefix(clean, "../") || strings.Contains(clean, "/../") {
		return "", fmt.Errorf("%q must be a relative asset path", p)
	}
	if !strings.HasPrefix(clean, "audio/music/") {
		return "", fmt.Errorf("%q must live under audio/music/", p)
	}
	if !strings.HasSuffix(strings.ToLower(clean), ".ogg") {
		return "", fmt.Errorf("%q is not a .ogg file", p)
	}
	return clean, nil
}

// StreamAsset is the metadata needed to stream one Ogg without decoding it into a
// resident PCM buffer.
type StreamAsset struct {
	Path              string `json:"path"`
	DurationMs        int64  `json:"durationMs"`
	Codec             string `json:"codec"`
	Channels          int    `json:"channels"`
	SampleRate        int    `json:"sampleRate"`
	TotalSamples      int64  `json:"totalSamples"`
	StreamChunkBytes  int    `json:"streamChunkBytes"`
	StreamRingChunks  int    `json:"streamRingChunks"`
	StreamBufferBytes int    `json:"streamBufferBytes"`
}

// StreamAssetIssue records a per-track metadata problem. The stream controller
// consumes this as a skip log rather than crashing a match.
type StreamAssetIssue struct {
	Path string `json:"path"`
	Rule string `json:"rule"`
	Msg  string `json:"msg"`
}

// StreamAssetIndex is a partial index: valid tracks are playable; invalid tracks
// are retained as structured errors so playlists can skip them and continue.
type StreamAssetIndex struct {
	assets map[string]StreamAsset
	errors map[string]StreamAssetIssue
	order  []string
}

// LoadStreamAssetIndex validates stream-track metadata from fsys. It opens each
// Ogg path and scans metadata through oggmeta.ParseReader, so it does not preload
// full decoded PCM or retain the compressed file.
func LoadStreamAssetIndex(fsys fs.FS, paths []string) StreamAssetIndex {
	idx := StreamAssetIndex{
		assets: make(map[string]StreamAsset, len(paths)),
		errors: make(map[string]StreamAssetIssue),
	}
	for _, raw := range paths {
		p, err := cleanMusicAssetPath(raw)
		if err != nil {
			idx.addIssue(raw, "MUSIC-PATH", err.Error())
			continue
		}
		if _, done := idx.assets[p]; done {
			continue
		}
		if _, done := idx.errors[p]; done {
			continue
		}
		f, err := fsys.Open(p)
		if err != nil {
			idx.addIssue(p, "AUD-READ", err.Error())
			continue
		}
		info, perr := oggmeta.ParseReader(f)
		cerr := f.Close()
		if perr != nil {
			idx.addIssue(p, "AUD-PARSE", perr.Error())
			continue
		}
		if cerr != nil {
			idx.addIssue(p, "AUD-READ", cerr.Error())
			continue
		}
		findings, resident := oggmeta.CheckLayout(info, oggmeta.CatMusic)
		if len(findings) > 0 {
			for _, finding := range findings {
				idx.addIssue(p, finding.Rule, finding.Msg)
			}
			continue
		}
		if resident {
			idx.addIssue(p, "AUD-STREAM", "music asset was classified resident; expected streamed")
			continue
		}
		if info.TotalSamples <= 0 || info.SampleRate <= 0 {
			idx.addIssue(p, "AUD-DURATION", fmt.Sprintf("non-positive duration metadata: samples=%d sampleRate=%d", info.TotalSamples, info.SampleRate))
			continue
		}
		durationMs := info.TotalSamples * 1000 / int64(info.SampleRate)
		if durationMs <= 0 {
			idx.addIssue(p, "AUD-DURATION", fmt.Sprintf("duration rounded to %d ms from samples=%d sampleRate=%d", durationMs, info.TotalSamples, info.SampleRate))
			continue
		}
		idx.assets[p] = StreamAsset{
			Path:              p,
			DurationMs:        durationMs,
			Codec:             info.Codec.String(),
			Channels:          info.Channels,
			SampleRate:        info.SampleRate,
			TotalSamples:      info.TotalSamples,
			StreamChunkBytes:  StreamChunkBytes,
			StreamRingChunks:  StreamRingChunks,
			StreamBufferBytes: StreamChunkBytes * StreamRingChunks,
		}
		idx.order = append(idx.order, p)
	}
	sort.Strings(idx.order)
	return idx
}

func (idx *StreamAssetIndex) addIssue(p, rule, msg string) {
	idx.errors[p] = StreamAssetIssue{Path: p, Rule: rule, Msg: msg}
}

// Lookup returns a stream asset by path.
func (idx StreamAssetIndex) Lookup(p string) (StreamAsset, bool) {
	a, ok := idx.assets[p]
	return a, ok
}

// Issue returns a recorded stream issue for path.
func (idx StreamAssetIndex) Issue(p string) (StreamAssetIssue, bool) {
	issue, ok := idx.errors[p]
	return issue, ok
}

// Assets returns valid stream paths in deterministic order.
func (idx StreamAssetIndex) Assets() []string {
	out := make([]string, len(idx.order))
	copy(out, idx.order)
	return out
}

// Issues returns invalid stream metadata findings in deterministic order.
func (idx StreamAssetIndex) Issues() []StreamAssetIssue {
	keys := make([]string, 0, len(idx.errors))
	for k := range idx.errors {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	out := make([]StreamAssetIssue, 0, len(keys))
	for _, k := range keys {
		out = append(out, idx.errors[k])
	}
	return out
}

// StreamKind identifies one of the two stream slots.
type StreamKind string

const (
	StreamMusic    StreamKind = "music"
	StreamAmbience StreamKind = "ambience"
)

type streamSlot struct {
	kind       StreamKind
	slot       int
	active     bool
	asset      StreamAsset
	posMs      int64
	loop       bool
	bufferFill int
	fade       *fadeState
}

type fadeState struct {
	asset      StreamAsset
	posMs      int64
	elapsedMs  int64
	durationMs int64
	startMs    int64
	endMs      int64
}

// StreamState is one stream slot's observable state.
type StreamState struct {
	Kind                StreamKind `json:"kind"`
	Slot                int        `json:"slot"`
	Active              bool       `json:"active"`
	Track               string     `json:"track,omitempty"`
	PositionMs          int64      `json:"positionMs,omitempty"`
	DurationMs          int64      `json:"durationMs,omitempty"`
	Loop                bool       `json:"loop,omitempty"`
	BufferFillBytes     int        `json:"bufferFillBytes,omitempty"`
	BufferCapacityBytes int        `json:"bufferCapacityBytes,omitempty"`
	Crossfade           bool       `json:"crossfade,omitempty"`
	NextTrack           string     `json:"nextTrack,omitempty"`
	NextPositionMs      int64      `json:"nextPositionMs,omitempty"`
	FadeElapsedMs       int64      `json:"fadeElapsedMs,omitempty"`
	FadeDurationMs      int64      `json:"fadeDurationMs,omitempty"`
}

// StreamLog is the durable FSV/audit event log for stream playback.
type StreamLog struct {
	AtMs              int64      `json:"atMs"`
	Kind              StreamKind `json:"kind"`
	Event             string     `json:"event"`
	Slot              int        `json:"slot"`
	Track             string     `json:"track,omitempty"`
	NextTrack         string     `json:"nextTrack,omitempty"`
	PositionMs        int64      `json:"positionMs,omitempty"`
	PositionBeforeMs  int64      `json:"positionBeforeMs,omitempty"`
	BufferFillBytes   int        `json:"bufferFillBytes,omitempty"`
	BufferBeforeBytes int        `json:"bufferBeforeBytes,omitempty"`
	FadeMs            int64      `json:"fadeMs,omitempty"`
	FadeStartMs       int64      `json:"fadeStartMs,omitempty"`
	FadeEndMs         int64      `json:"fadeEndMs,omitempty"`
	RNGDraws          int        `json:"rngDraws,omitempty"`
	Rule              string     `json:"rule,omitempty"`
	Detail            string     `json:"detail,omitempty"`
}

// StreamSnapshot is the controller source of truth for FSV.
type StreamSnapshot struct {
	NowMs         int64         `json:"nowMs"`
	MapID         string        `json:"mapId"`
	Faction       string        `json:"faction"`
	CrossfadeMs   int           `json:"crossfadeMs"`
	Shuffle       bool          `json:"shuffle"`
	RNGDraws      int           `json:"rngDraws"`
	ActiveStreams int           `json:"activeStreams"`
	Streams       []StreamState `json:"streams"`
	Logs          []StreamLog   `json:"logs"`
}

// StreamController owns the presentation-only music/ambience stream state. It
// has no sim dependency; shuffle draws come from an injected math/rand instance.
type StreamController struct {
	selection MusicSelection
	assets    StreamAssetIndex
	rng       *rand.Rand

	nowMs     int64
	order     []string
	nextTrack int
	rngDraws  int
	music     streamSlot
	ambience  streamSlot
	logs      []StreamLog
}

// NewStreamController constructs a stream controller. If rng is nil, a
// presentation-only time seed is used; never pass the sim PRNG here.
func NewStreamController(sel MusicSelection, assets StreamAssetIndex, rng *rand.Rand) *StreamController {
	if rng == nil {
		rng = rand.New(rand.NewSource(time.Now().UnixNano()))
	}
	return &StreamController{selection: sel, assets: assets, rng: rng}
}

// Start resolves the ambience and first music track, skipping invalid assets
// with structured logs instead of crashing.
func (c *StreamController) Start() {
	c.nowMs = 0
	c.nextTrack = 0
	c.rngDraws = 0
	c.logs = nil
	c.music = streamSlot{kind: StreamMusic, slot: MusicStreamSlot}
	c.ambience = streamSlot{kind: StreamAmbience, slot: AmbienceStreamSlot}
	c.prepareOrder()
	c.startAmbience()
	c.startInitialMusic()
}

func (c *StreamController) prepareOrder() {
	c.order = append([]string(nil), c.selection.Tracks...)
	if c.selection.Shuffle && len(c.order) > 1 {
		for i := len(c.order) - 1; i > 0; i-- {
			j := c.rng.Intn(i + 1)
			c.rngDraws++
			c.order[i], c.order[j] = c.order[j], c.order[i]
		}
		c.logs = append(c.logs, StreamLog{
			AtMs:     c.nowMs,
			Kind:     StreamMusic,
			Event:    "shuffle",
			Slot:     MusicStreamSlot,
			RNGDraws: c.rngDraws,
			Detail:   strings.Join(c.order, ","),
		})
	}
}

func (c *StreamController) startAmbience() {
	a, ok := c.assets.Lookup(c.selection.Ambience)
	if !ok {
		c.logSkip(StreamAmbience, AmbienceStreamSlot, c.selection.Ambience)
		return
	}
	c.ambience = streamSlot{
		kind:       StreamAmbience,
		slot:       AmbienceStreamSlot,
		active:     true,
		asset:      a,
		loop:       c.selection.AmbienceLoop,
		bufferFill: a.StreamBufferBytes,
	}
	c.logs = append(c.logs, c.slotLog(&c.ambience, "play"))
}

func (c *StreamController) startInitialMusic() {
	a, ok := c.nextValidTrack()
	if !ok {
		return
	}
	c.music = streamSlot{
		kind:       StreamMusic,
		slot:       MusicStreamSlot,
		active:     true,
		asset:      a,
		bufferFill: a.StreamBufferBytes,
	}
	c.logs = append(c.logs, c.slotLog(&c.music, "play"))
}

func (c *StreamController) nextValidTrack() (StreamAsset, bool) {
	if len(c.order) == 0 {
		c.logs = append(c.logs, StreamLog{AtMs: c.nowMs, Kind: StreamMusic, Event: "skip", Slot: MusicStreamSlot, Rule: "MUSIC-EMPTY", Detail: "playlist has no tracks"})
		return StreamAsset{}, false
	}
	for tries := 0; tries < len(c.order); tries++ {
		p := c.order[c.nextTrack%len(c.order)]
		c.nextTrack = (c.nextTrack + 1) % len(c.order)
		if a, ok := c.assets.Lookup(p); ok {
			return a, true
		}
		c.logSkip(StreamMusic, MusicStreamSlot, p)
	}
	c.logs = append(c.logs, StreamLog{AtMs: c.nowMs, Kind: StreamMusic, Event: "skip", Slot: MusicStreamSlot, Rule: "MUSIC-NO-VALID", Detail: "no valid tracks in playlist"})
	return StreamAsset{}, false
}

func (c *StreamController) logSkip(kind StreamKind, slot int, track string) {
	l := StreamLog{AtMs: c.nowMs, Kind: kind, Event: "skip", Slot: slot, Track: track}
	if issue, ok := c.assets.Issue(track); ok {
		l.Rule = issue.Rule
		l.Detail = issue.Msg
	} else {
		l.Rule = "MUSIC-MISSING"
		l.Detail = "missing stream metadata"
	}
	c.logs = append(c.logs, l)
}

func (c *StreamController) slotLog(s *streamSlot, event string) StreamLog {
	return StreamLog{
		AtMs:            c.nowMs,
		Kind:            s.kind,
		Event:           event,
		Slot:            s.slot,
		Track:           s.asset.Path,
		PositionMs:      s.posMs,
		BufferFillBytes: s.bufferFill,
	}
}

// Advance moves stream playback forward by deltaMs and records loop/crossfade
// state transitions. Non-positive deltas are ignored.
func (c *StreamController) Advance(deltaMs int64) {
	if deltaMs <= 0 {
		return
	}
	c.nowMs += deltaMs
	c.advanceAmbience(deltaMs)
	c.advanceMusic(deltaMs)
}

func (c *StreamController) advanceAmbience(deltaMs int64) {
	s := &c.ambience
	if !s.active {
		return
	}
	before := s.posMs
	s.posMs += deltaMs
	if s.loop && s.asset.DurationMs > 0 && s.posMs >= s.asset.DurationMs {
		s.posMs %= s.asset.DurationMs
		c.logs = append(c.logs, StreamLog{
			AtMs:             c.nowMs,
			Kind:             s.kind,
			Event:            "loop",
			Slot:             s.slot,
			Track:            s.asset.Path,
			PositionBeforeMs: before + deltaMs,
			PositionMs:       s.posMs,
			BufferFillBytes:  s.bufferFill,
		})
	}
}

func (c *StreamController) advanceMusic(deltaMs int64) {
	s := &c.music
	if !s.active {
		return
	}
	s.posMs += deltaMs
	if s.fade != nil {
		s.fade.posMs += deltaMs
		s.fade.elapsedMs += deltaMs
		if s.fade.elapsedMs >= s.fade.durationMs {
			old := s.asset.Path
			next := s.fade.asset
			nextPos := s.fade.posMs
			s.asset = next
			s.posMs = nextPos
			s.fade = nil
			c.logs = append(c.logs, StreamLog{
				AtMs:            c.nowMs,
				Kind:            StreamMusic,
				Event:           "crossfade-end",
				Slot:            MusicStreamSlot,
				Track:           old,
				NextTrack:       next.Path,
				PositionMs:      s.posMs,
				BufferFillBytes: s.bufferFill,
			})
		}
		return
	}
	if c.selection.CrossfadeMs > 0 && s.asset.DurationMs > int64(c.selection.CrossfadeMs) && s.posMs >= s.asset.DurationMs-int64(c.selection.CrossfadeMs) {
		c.startCrossfade()
		return
	}
	if s.posMs >= s.asset.DurationMs {
		c.switchMusic("track-end")
	}
}

func (c *StreamController) startCrossfade() {
	next, ok := c.nextValidTrack()
	if !ok {
		return
	}
	fadeMs := int64(c.selection.CrossfadeMs)
	c.music.fade = &fadeState{
		asset:      next,
		durationMs: fadeMs,
		startMs:    c.nowMs,
		endMs:      c.nowMs + fadeMs,
	}
	c.logs = append(c.logs, StreamLog{
		AtMs:            c.nowMs,
		Kind:            StreamMusic,
		Event:           "crossfade-start",
		Slot:            MusicStreamSlot,
		Track:           c.music.asset.Path,
		NextTrack:       next.Path,
		PositionMs:      c.music.posMs,
		BufferFillBytes: c.music.bufferFill,
		FadeMs:          fadeMs,
		FadeStartMs:     c.music.fade.startMs,
		FadeEndMs:       c.music.fade.endMs,
	})
}

func (c *StreamController) switchMusic(event string) {
	old := c.music.asset.Path
	next, ok := c.nextValidTrack()
	if !ok {
		c.music.active = false
		return
	}
	c.music.asset = next
	c.music.posMs = 0
	c.music.bufferFill = next.StreamBufferBytes
	c.logs = append(c.logs, StreamLog{
		AtMs:            c.nowMs,
		Kind:            StreamMusic,
		Event:           event,
		Slot:            MusicStreamSlot,
		Track:           old,
		NextTrack:       next.Path,
		PositionMs:      c.music.posMs,
		BufferFillBytes: c.music.bufferFill,
	})
}

// ReportUnderrun records an underrun and refills the bounded stream ring. A real
// backend calls this when it observes an empty queue; tests use it as the public
// recovery path instead of reaching into controller internals.
func (c *StreamController) ReportUnderrun(kind StreamKind, observedFill int) {
	s := c.slot(kind)
	if s == nil || !s.active {
		c.logs = append(c.logs, StreamLog{AtMs: c.nowMs, Kind: kind, Event: "underrun-ignored", Detail: "stream inactive"})
		return
	}
	if observedFill < 0 {
		observedFill = 0
	}
	if observedFill > s.asset.StreamBufferBytes {
		observedFill = s.asset.StreamBufferBytes
	}
	before := observedFill
	s.bufferFill = s.asset.StreamBufferBytes
	c.logs = append(c.logs, StreamLog{
		AtMs:              c.nowMs,
		Kind:              kind,
		Event:             "underrun-recover",
		Slot:              s.slot,
		Track:             s.asset.Path,
		PositionMs:        s.posMs,
		BufferBeforeBytes: before,
		BufferFillBytes:   s.bufferFill,
	})
}

func (c *StreamController) slot(kind StreamKind) *streamSlot {
	switch kind {
	case StreamMusic:
		return &c.music
	case StreamAmbience:
		return &c.ambience
	default:
		return nil
	}
}

// Dump returns a deep copy of the controller state.
func (c *StreamController) Dump() StreamSnapshot {
	streams := []StreamState{stateOf(c.music), stateOf(c.ambience)}
	active := 0
	for _, s := range streams {
		if s.Active {
			active++
		}
	}
	logs := make([]StreamLog, len(c.logs))
	copy(logs, c.logs)
	return StreamSnapshot{
		NowMs:         c.nowMs,
		MapID:         c.selection.MapID,
		Faction:       c.selection.Faction,
		CrossfadeMs:   c.selection.CrossfadeMs,
		Shuffle:       c.selection.Shuffle,
		RNGDraws:      c.rngDraws,
		ActiveStreams: active,
		Streams:       streams,
		Logs:          logs,
	}
}

func stateOf(s streamSlot) StreamState {
	st := StreamState{
		Kind:                s.kind,
		Slot:                s.slot,
		Active:              s.active,
		Loop:                s.loop,
		BufferFillBytes:     s.bufferFill,
		BufferCapacityBytes: s.asset.StreamBufferBytes,
	}
	if !s.active {
		return st
	}
	st.Track = s.asset.Path
	st.PositionMs = s.posMs
	st.DurationMs = s.asset.DurationMs
	if s.fade != nil {
		st.Crossfade = true
		st.NextTrack = s.fade.asset.Path
		st.NextPositionMs = s.fade.posMs
		st.FadeElapsedMs = s.fade.elapsedMs
		st.FadeDurationMs = s.fade.durationMs
	}
	return st
}
