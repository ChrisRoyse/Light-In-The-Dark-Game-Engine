package main

import (
	"encoding/base64"
	"os"
	"path/filepath"
	"strings"
	"testing"

	litlocale "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	litaudio "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
	litinput "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/input"
	litrender "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render"
	lithud "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render/hud"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/g3n/engine/core"
)

func TestResolutionFlagSetFSV(t *testing.T) {
	var r resolutionFlag
	if err := r.Set("1920x1080"); err != nil {
		t.Fatalf("valid resolution rejected: %v", err)
	}
	t.Logf("FSV resolution valid BEFORE empty AFTER %+v", r)
	if r.W != 1920 || r.H != 1080 || !r.set {
		t.Fatalf("valid resolution parsed incorrectly: %+v", r)
	}

	before := r
	invalid := []string{
		"",
		"1920",
		"1920x",
		"x1080",
		"1920x1080extra",
		"1920x1080x1",
		"0x1080",
		"1920x-1",
		"1920X1080",
	}
	for _, input := range invalid {
		if err := r.Set(input); err == nil {
			t.Fatalf("invalid resolution %q accepted: %+v", input, r)
		}
		t.Logf("FSV resolution invalid input=%q BEFORE %+v AFTER %+v", input, before, r)
		if r != before {
			t.Fatalf("invalid resolution %q mutated state: got %+v want %+v", input, r, before)
		}
	}
}

func TestCameraZoomRequestFSV(t *testing.T) {
	cfg := litrender.DefaultRTSCameraConfig(16.0 / 9.0)
	cases := []struct {
		input string
		want  float32
	}{
		{input: "", want: cfg.Zoom},
		{input: "default", want: cfg.Zoom},
		{input: "min", want: cfg.ZoomMin},
		{input: "zmin", want: cfg.ZoomMin},
		{input: "max", want: cfg.ZoomMax},
		{input: "zmax", want: cfg.ZoomMax},
		{input: "below-min", want: cfg.ZoomMin * 0.5},
		{input: "above-max", want: cfg.ZoomMax * 2},
		{input: "1700", want: 1700},
	}
	for _, tc := range cases {
		got, err := cameraZoomRequest(tc.input, cfg)
		t.Logf("FSV camera zoom request input=%q got=%.3f err=%v", tc.input, got, err)
		if err != nil || got != tc.want {
			t.Fatalf("cameraZoomRequest(%q) = %.3f, %v; want %.3f nil", tc.input, got, err, tc.want)
		}
	}
	if got, err := cameraZoomRequest("bogus", cfg); err == nil {
		t.Fatalf("invalid zoom accepted: got %.3f", got)
	} else {
		t.Logf("FSV camera invalid zoom input=%q err=%v", "bogus", err)
	}
}

func TestBuildCameraProjectionModeFSV(t *testing.T) {
	persp, err := buildCamera(960, 540, "default", "persp")
	if err != nil {
		t.Fatalf("perspective camera rejected: %v", err)
	}
	ortho, err := buildCamera(960, 540, "above-max", "ortho")
	if err != nil {
		t.Fatalf("orthographic camera rejected: %v", err)
	}
	t.Logf("FSV renderdemo camera persp=%+v", persp.Snapshot())
	t.Logf("FSV renderdemo camera ortho=%+v", ortho.Snapshot())

	if persp.Snapshot().Projection != "perspective" {
		t.Fatalf("perspective camera flag produced wrong projection: %+v", persp.Snapshot())
	}
	orthoSnap := ortho.Snapshot()
	if orthoSnap.Projection != "orthographic" || orthoSnap.Zoom != orthoSnap.ZoomMax || !litrenderClose32(orthoSnap.OrthoSize, orthoSnap.OrthoSizeMax) {
		t.Fatalf("orthographic camera flag did not clamp zoom to Size_max: %+v", orthoSnap)
	}
	if _, err := buildCamera(960, 540, "default", "isometric"); err == nil {
		t.Fatalf("invalid camera projection accepted")
	} else {
		t.Logf("FSV renderdemo invalid camera projection err=%v", err)
	}
}

func TestBuildAudioInitDumpNullFSV(t *testing.T) {
	before := litaudio.NewManager(nil).Dump()
	dump, err := buildAudioInitDump("null")
	if err != nil {
		t.Fatalf("audio init null rejected: %v", err)
	}
	if !dump.OK {
		t.Fatalf("audio init null failed: %+v", dump.Errors)
	}
	if dump.Backend != "null" || dump.BackendSources != 0 || dump.Snapshot.BackendSources != 0 {
		t.Fatalf("null backend/source SoT mismatch: %+v", dump)
	}
	if dump.AccountingMaxVoices != litaudio.MaxVoices || dump.Snapshot.MaxVoices != litaudio.MaxVoices || dump.Snapshot.VoiceCount != 3 {
		t.Fatalf("accounting counts wrong: %+v", dump.Snapshot)
	}
	if !dump.ListenerMatchesFocus || dump.ListenerMatchesEye || dump.Listener != dump.CameraFocus {
		t.Fatalf("listener should bind camera focus, not eye: focus=%+v eye=%+v listener=%+v", dump.CameraFocus, dump.CameraEye, dump.Listener)
	}
	if !dump.AccountingMatchesNull || dump.NullAccountingHash != dump.BackendAccountingHash {
		t.Fatalf("null accounting should self-match: null=%s backend=%s", dump.NullAccountingHash, dump.BackendAccountingHash)
	}
	if !dump.PanSignFlipped || len(dump.PanTrace) != 2 || dump.PanTrace[0].Pan <= 0 || dump.PanTrace[1].Pan >= 0 {
		t.Fatalf("pan trace did not flip sign: %+v", dump.PanTrace)
	}
	if !dump.SimHash.Equal || dump.SimHash.AudioCalls != 2 {
		t.Fatalf("audio-on/off hash pair wrong: %+v", dump.SimHash)
	}
	t.Logf("FSV #227 renderdemo audio-init null BEFORE backend=%s sources=%d voices=%d AFTER backend=%s sources=%d voices=%d listener=%+v focus=%+v eye=%+v pan=%+.3f→%+.3f hash=%s",
		before.Backend, before.BackendSources, before.VoiceCount,
		dump.Backend, dump.BackendSources, dump.Snapshot.VoiceCount,
		dump.Listener, dump.CameraFocus, dump.CameraEye, dump.PanTrace[0].Pan, dump.PanTrace[1].Pan, dump.SimHash.AudioOn)
}

func TestBuildAudioInitDumpInvalidModeFSV(t *testing.T) {
	dump, err := buildAudioInitDump("bogus")
	if err == nil {
		t.Fatalf("invalid audio backend mode accepted: %+v", dump)
	}
	if dump.OK || len(dump.Errors) == 0 {
		t.Fatalf("invalid mode should produce a failed dump with an error: %+v", dump)
	}
	t.Logf("FSV #227 renderdemo audio-init invalid mode BEFORE mode=%q AFTER ok=%v errors=%v", "bogus", dump.OK, dump.Errors)
}

func TestBuildGroupFSVFSV(t *testing.T) {
	rig, err := buildCamera(960, 540, "default", "persp")
	if err != nil {
		t.Fatalf("camera rejected: %v", err)
	}
	dump := buildGroupFSV(core.NewNode(), rig)
	t.Logf("FSV renderdemo groups ok=%v current=%s selection=%v center=(%.1f,%.1f) cameraAnchor=%+v",
		dump.OK, dump.Current.Name, dump.Current.Selection, dump.Current.CenterX, dump.Current.CenterZ, rig.Snapshot().Anchor)
	if !dump.OK || dump.Current.Name != "doubletap-299" || !dump.Current.CenterRequested || dump.Current.CenterX != 120 || dump.Current.CenterZ != 80 {
		t.Fatalf("group FSV current mismatch: %+v", dump.Current)
	}
	if rig.Snapshot().Anchor.X != 120 || rig.Snapshot().Anchor.Z != 80 {
		t.Fatalf("double-tap did not center camera: %+v", rig.Snapshot().Anchor)
	}
	seen := map[string]groupCaseDump{}
	for _, c := range dump.Cases {
		seen[c.Name] = c
		if !c.OK || c.CommandRecordsEmitted != 0 {
			t.Fatalf("case %s failed or emitted commands: %+v", c.Name, c)
		}
	}
	if seen["recall-pruned"].Pruned != 2 || seen["doubletap-350"].CenterRequested || seen["generation-reuse"].RecycledID != 0x01000007 {
		t.Fatalf("group FSV edge cases missing: recall=%+v late=%+v gen=%+v",
			seen["recall-pruned"], seen["doubletap-350"], seen["generation-reuse"])
	}
}

func TestBuildSmartOrderFSV(t *testing.T) {
	defer chdirRepoRoot(t)()
	dump, err := buildSmartOrderFSV(core.NewNode())
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV renderdemo smart orders ok=%v current=%+v cases=%+v", dump.OK, dump.Current, dump.Cases)
	if !dump.OK || dump.Current.Name != "harvest-split" || len(dump.Current.Records) != 2 {
		t.Fatalf("smart-order fixture current mismatch: %+v", dump)
	}
	if dump.Current.Records[0].Opcode != sim.OpHarvest || dump.Current.Records[1].Opcode != sim.OpMove {
		t.Fatalf("harvest split should emit harvest then move: %+v", dump.Current.Records)
	}
	if len(dump.Current.Records[0].Units) != 2 || len(dump.Current.Records[1].Units) != 1 {
		t.Fatalf("harvest split unit groups wrong: %+v", dump.Current.Records)
	}
	seen := map[string]orderCaseDump{}
	for _, c := range dump.Cases {
		seen[c.Name] = c
	}
	if seen["hidden-enemy"].Feedback != litinput.SmartFeedbackHiddenTarget.String() || len(seen["hidden-enemy"].Records) != 0 {
		t.Fatalf("hidden target edge wrong: %+v", seen["hidden-enemy"])
	}
	if seen["dead-target"].Feedback != litinput.SmartFeedbackDeadTarget.String() || len(seen["dead-target"].Records) != 0 {
		t.Fatalf("dead target edge wrong: %+v", seen["dead-target"])
	}
}

func TestBuildQueueFSV(t *testing.T) {
	dump, err := buildQueueFSV(core.NewNode())
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV renderdemo queue ok=%v flag=%02x replay=%+v screenshot=%+v final=%+v cases=%+v",
		dump.OK, dump.QueuedFlagByte, dump.Replay, dump.ScreenshotState, dump.FinalState, dump.Cases)
	if !dump.OK {
		t.Fatalf("queue fixture failed: %+v", dump.Errors)
	}
	if dump.QueuedFlagByte != sim.CmdFlagQueued || dump.QueuedFlagHex == "" {
		t.Fatalf("queued flag hexdump missing/wrong: byte=%02x hex=%q", dump.QueuedFlagByte, dump.QueuedFlagHex)
	}
	if len(dump.Trace) < 2 || dump.Trace[1].QueueDepth != 4 {
		t.Fatalf("queue did not grow to four shifted entries: trace=%+v", dump.Trace)
	}
	if dump.ScreenshotState.QueueDepth > 2 || dump.ScreenshotState.MoveState != sim.MoveFollowing || dump.ScreenshotState.Pos == dump.ScreenshotState.Current.Point {
		t.Fatalf("screenshot state is not mid-route after queued drain: %+v", dump.ScreenshotState)
	}
	if !dump.Replay.Equal || dump.Replay.FirstHash == "" || dump.Replay.FirstHash != dump.Replay.SecondHash {
		t.Fatalf("replay hash equality missing: %+v", dump.Replay)
	}
	if dump.FinalState.Pos != dump.SecondSequence[0] {
		t.Fatalf("final position should be second sequence target: got %+v want %+v", dump.FinalState.Pos, dump.SecondSequence[0])
	}
	seen := map[string]queueCaseDump{}
	for _, c := range dump.Cases {
		seen[c.Name] = c
		if !c.OK {
			t.Fatalf("queue case %s failed: %+v", c.Name, c)
		}
	}
	if seen["overflow-20-shift-orders"].After.QueueDepth != sim.MaxOrderQueue || len(seen["overflow-20-shift-orders"].Drops) != 4 {
		t.Fatalf("overflow edge wrong: %+v", seen["overflow-20-shift-orders"])
	}
	if seen["unmodified-collapse"].After.QueueDepth != 0 || seen["unmodified-collapse"].After.TotalOrders != 1 {
		t.Fatalf("collapse edge wrong: %+v", seen["unmodified-collapse"])
	}
	if seen["dead-unit-cleanup"].After.Alive || seen["dead-unit-cleanup"].After.TotalOrders != 0 ||
		seen["dead-unit-cleanup"].After.OrderPoolFree != seen["dead-unit-cleanup"].PoolFreeBase {
		t.Fatalf("dead cleanup edge wrong: %+v", seen["dead-unit-cleanup"])
	}
}

func TestBuildCommandCardKeymapFSV(t *testing.T) {
	defer chdirRepoRoot(t)()
	dir := t.TempDir()
	path := filepath.Join(dir, "custom.toml")
	if err := os.WriteFile(path, []byte("profile = \"grid\"\n[game]\n\"card.slot.0\" = [\"T\"]\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	localeTable := mustRenderDemoLocale(t)
	dump, display, err := buildCommandCardFSV(localeTable, "unit", path)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV renderdemo keymap profile=%s summary=%q keypresses=%+v", dump.KeymapProfile, display.Summary.String(), dump.KeyPresses)
	if display.Slots[0].Hotkey != "T" || len(dump.KeyPresses) != 2 {
		t.Fatalf("custom keymap did not relabel slot0 or emit keypresses: hotkey=%q presses=%+v", display.Slots[0].Hotkey, dump.KeyPresses)
	}
	if dump.KeyPresses[0].Key != "T" || !dump.KeyPresses[0].Accepted || dump.KeyPresses[0].Emitted == nil || dump.KeyPresses[0].Emitted.Opcode != sim.OpMove {
		t.Fatalf("T did not emit slot0 command: %+v", dump.KeyPresses[0])
	}
	if dump.KeyPresses[1].Key != "Q" || dump.KeyPresses[1].Accepted || dump.KeyPresses[1].Reason != "unbound" {
		t.Fatalf("Q should be unbound after Q->T rebind: %+v", dump.KeyPresses[1])
	}
}

func TestBuildMapDataDumpFSV(t *testing.T) {
	defer chdirRepoRoot(t)()
	dump, err := buildMapDataDump("data/maps/test64")
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV renderdemo mapdata fp=%s counts=%+v samples=%+v", dump.Fingerprint, dump.Counts, dump.PathingSamples)
	if !dump.OK || dump.Width != 64 || dump.Height != 64 || dump.PathingWidth != 256 || dump.Counts.Water != 512 {
		t.Fatalf("map dump metadata/counts wrong: %+v", dump)
	}
	if len(dump.PathingSamples) < 5 || dump.PathingSamples[1].Flags != 4 || dump.PathingSamples[2].CliffText != "r0" || dump.PathingSamples[3].CliffText != "1" {
		t.Fatalf("map dump samples wrong: %+v", dump.PathingSamples)
	}
	if len(dump.HeightSamples) < 3 || dump.HeightSamples[0].Height != 0 || dump.HeightSamples[1].Height != 256 || dump.HeightSamples[2].Height != 512 {
		t.Fatalf("map dump height samples wrong: %+v", dump.HeightSamples)
	}
	if len(dump.SplatSamples) < 2 || dump.SplatSamples[1].Weight.C != 255 {
		t.Fatalf("map dump splat samples wrong: %+v", dump.SplatSamples)
	}
}

func TestBuildCampaignMenuRuntimeFSV(t *testing.T) {
	defer chdirRepoRoot(t)()
	canvas, err := lithud.NewCanvas(1366, 768, 1)
	if err != nil {
		t.Fatal(err)
	}
	table := mustRenderDemoLocale(t)
	for _, scenario := range []string{"campaign-select", "fresh", "unlocked", "save-load", "missing-archive"} {
		dump, err := buildCampaignMenuRuntime(canvas, scenario, "en", table)
		if err != nil {
			t.Fatalf("%s: %v", scenario, err)
		}
		t.Logf("FSV renderdemo campaign scenario=%s screen=%s ok=%v store=%+v labels=%+v errors=%v",
			scenario, dump.Screen, dump.OK, dump.AfterStore, dump.Layout.Labels, dump.Errors)
		if !dump.OK || len(dump.Layout.Issues) != 0 || dump.AfterStore.Bytes == 0 || dump.AfterStore.SHA256 == "" {
			t.Fatalf("%s dump not OK: %+v", scenario, dump)
		}
		switch scenario {
		case "campaign-select":
			if dump.Catalog == nil || dump.Catalog.SelectedCampaignID != "vigil-render" || dump.Screen != lithud.CampaignMenuScreenCampaignSelect {
				t.Fatalf("campaign select wrong: %+v", dump)
			}
		case "fresh":
			if dump.View == nil || dump.View.Missions[0].Status != "available" || dump.View.Missions[1].Status != "locked" {
				t.Fatalf("fresh statuses wrong: %+v", dump.View)
			}
		case "unlocked":
			if dump.BeforeStore == nil || dump.View == nil || dump.View.Missions[0].Status != "complete" || dump.View.Missions[1].Status != "available" ||
				len(dump.View.CarryOver.Heroes) != 1 || dump.View.CarryOver.Heroes[0].Name != "Ser Caldus" {
				t.Fatalf("unlocked carry-over wrong: %+v", dump)
			}
		case "save-load":
			if !dump.CheckpointRead || dump.Checkpoint != "inside-the-gate" || dump.View == nil || dump.View.CarryOver.Heroes[0].Name != "Mira Vale" {
				t.Fatalf("save-load checkpoint/carry wrong: %+v", dump)
			}
		case "missing-archive":
			if dump.View == nil || dump.View.Missions[0].Status != "missing-archive" || !strings.Contains(dump.View.Missions[0].Error, "worlds/m1.litdworld") {
				t.Fatalf("missing archive wrong: %+v", dump.View)
			}
		}
	}
}

const renderdemoMonoVorbis1sB64 = "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"

const renderdemoStereoVorbis2sB64 = "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"

func mustDecodeB64(t *testing.T, s string) []byte {
	t.Helper()
	b, err := base64.StdEncoding.DecodeString(s)
	if err != nil {
		t.Fatal(err)
	}
	return b
}

func TestBuildAudioLoadDumpFSV(t *testing.T) {
	dir := t.TempDir()
	mustWrite := func(rel string, data []byte) {
		t.Helper()
		p := filepath.Join(dir, filepath.FromSlash(rel))
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, data, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	mustWrite("sfx/hit.ogg", mustDecodeB64(t, renderdemoMonoVorbis1sB64))
	mustWrite("music/theme.ogg", mustDecodeB64(t, renderdemoStereoVorbis2sB64))
	dump, err := buildAudioLoadDump(dir)
	if err != nil {
		t.Fatalf("audio dump rejected: %v %+v", err, dump)
	}
	if !dump.OK || dump.AssetCount != 2 || dump.ResidentDecodedBytes != 88200 {
		t.Fatalf("audio dump SoT mismatch: %+v", dump)
	}
	if dump.StreamBufferBytes != 2*64*1024 {
		t.Fatalf("music stream ring bytes wrong: %+v", dump)
	}
	t.Logf("FSV renderdemo audio dump: resident=%d streamBuffer=%d assets=%d", dump.ResidentDecodedBytes, dump.StreamBufferBytes, dump.AssetCount)
}

func TestBuildTerrainFSV(t *testing.T) {
	defer chdirRepoRoot(t)()
	scene := core.NewNode()
	spec, dump, err := buildTerrainFSV(scene, "terrain-units", true)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV renderdemo terrain spec=%+v triangles=%d maxDiff=%d inverted=%d border=%+v units=%+v",
		spec, dump.TriangleCount, dump.MaxHeightDiff, dump.InvertedTriangles, dump.BorderVertices, dump.Units)
	if !dump.OK || dump.VertexCount != 4225 || dump.TriangleCount != 8192 || dump.MaxHeightDiff != 0 || dump.InvertedTriangles != 0 {
		t.Fatalf("terrain dump wrong: %+v", dump)
	}
	if len(dump.HeightSamples) != 100 || len(dump.BorderVertices) != 4 || len(dump.Units) != 4 {
		t.Fatalf("terrain FSV coverage wrong: samples=%d border=%d units=%d", len(dump.HeightSamples), len(dump.BorderVertices), len(dump.Units))
	}
	if spec.expected.VisibleGraphics != 5 || spec.expected.OpaqueDrawCalls != 5 {
		t.Fatalf("terrain-units expected stats wrong: %+v", spec.expected)
	}
}

func TestBuildTerrainChunksFSV(t *testing.T) {
	defer chdirRepoRoot(t)()
	scene := core.NewNode()
	spec, dump, err := buildTerrainChunksFSV(scene, false)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV renderdemo terrain-chunks spec=%+v count=%d cols=%d rows=%d maxTris=%d totalTris=%d seams=%d border=%+v",
		spec.name, dump.ChunkCount, dump.ChunkCols, dump.ChunkRows, dump.MaxChunkTris, dump.TriangleCount, dump.SeamMismatches, dump.BorderVertices)
	if !dump.OK || !dump.Chunked {
		t.Fatalf("chunk dump not OK: %+v", dump)
	}
	if dump.ChunkCount != 16 || dump.ChunkCols != 4 || dump.ChunkRows != 4 {
		t.Fatalf("chunk grid wrong: count=%d cols=%d rows=%d", dump.ChunkCount, dump.ChunkCols, dump.ChunkRows)
	}
	if dump.MaxChunkTris != 512 || dump.TriangleCount != 8192 {
		t.Fatalf("chunk tris wrong: max=%d total=%d", dump.MaxChunkTris, dump.TriangleCount)
	}
	if dump.SeamMismatches != 0 {
		t.Fatalf("seam mismatches=%d, want 0 (cracks)", dump.SeamMismatches)
	}
	if len(dump.BorderVertices) != 4 || len(dump.ChunkTris) != 16 {
		t.Fatalf("coverage wrong: border=%d chunkTris=%d", len(dump.BorderVertices), len(dump.ChunkTris))
	}
}

func TestBuildSpellstormFSV(t *testing.T) {
	scene := core.NewNode()
	_, dump, err := buildSpellstormFSV(scene, false)
	if err != nil {
		t.Fatal(err)
	}
	last := dump.Events[len(dump.Events)-1].Decision
	t.Logf("FSV spellstorm maxActive=%d finalActive=%d events=%d lastEvict=%+v OK=%v",
		dump.MaxActive, dump.FinalActive, len(dump.Events), last, dump.OK)
	if !dump.OK || dump.MaxActive != 8 || dump.FinalActive != 8 || len(dump.Events) != 9 {
		t.Fatalf("spellstorm dump wrong: %+v", dump)
	}
	if !last.Granted || last.Victim < 0 || last.Reason != "evict:lower-priority" {
		t.Fatalf("9th request must evict a lower-priority light: %+v", last)
	}

	sceneLow := core.NewNode()
	_, low, err := buildSpellstormFSV(sceneLow, true)
	if err != nil {
		t.Fatal(err)
	}
	lastLow := low.Events[len(low.Events)-1].Decision
	t.Logf("FSV spellstorm low-preset finalActive=%d lastReason=%s OK=%v", low.FinalActive, lastLow.Reason, low.OK)
	if !low.OK || low.FinalActive != 0 || lastLow.Reason != "denied:low-preset" {
		t.Fatalf("low preset must bind no lights: %+v", low)
	}
}

func chdirRepoRoot(t *testing.T) func() {
	t.Helper()
	cwd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Chdir("../.."); err != nil {
		t.Fatal(err)
	}
	return func() {
		if err := os.Chdir(cwd); err != nil {
			t.Fatal(err)
		}
	}
}

func mustRenderDemoLocale(t *testing.T) *litlocale.Table {
	t.Helper()
	table, err := litlocale.Load(os.DirFS("data"), "en")
	if err != nil {
		t.Fatal(err)
	}
	return table
}

func litrenderClose32(got, want float32) bool {
	d := got - want
	if d < 0 {
		d = -d
	}
	return d <= 0.001
}
