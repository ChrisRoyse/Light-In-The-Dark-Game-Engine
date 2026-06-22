// Command renderdemo renders deterministic primitive scenes for render-stat FSV.
//
// Usage:
//
//	renderdemo -scene counted -autotest -shot artifacts/stats-hud.png -dump artifacts/stats.json
//	renderdemo -scene camera-rig -camera ortho -autotest -shot artifacts/ortho-zmax.png -dump artifacts/ortho.json
//	renderdemo -hud -res 1920x1080 -autotest -shot artifacts/canvas.png -dump artifacts/canvas.json
//	renderdemo -hud -scene campaign-menu -campaign-scenario unlocked -autotest -shot artifacts/campaign-menu.png -dump artifacts/campaign-menu.json
//
// Scenes are synthetic and hand-countable. Each scene includes one GUI label
// so screenshots show a stats line; world counts remain separated in the JSON
// via opaque/transparent/gui buckets.
package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/json"
	"flag"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing/fstest"
	"time"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	litasset "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset"
	litlocale "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	litaudio "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
	litcampaign "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/campaign"
	litdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	litinput "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/input"
	litrender "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render"
	lithud "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render/hud"
	litterrain "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render/terrain"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
	"github.com/g3n/engine/app"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/gui"
	"github.com/g3n/engine/light"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
	"github.com/g3n/engine/renderer"
	"github.com/g3n/engine/texture"
	"github.com/g3n/engine/window"
)

const (
	defaultWidth  = 960
	defaultHeight = 540
)

type sceneSpec struct {
	name     string
	expected litrender.FrameStats
}

type renderDemoDump struct {
	litrender.FrameStats
	Scene     string                  `json:"scene"`
	Camera    litrender.RTSCameraDump `json:"camera"`
	Selection *selectionRuntimeDump   `json:"selection,omitempty"`
	Groups    *groupRuntimeDump       `json:"groups,omitempty"`
	Orders    *orderRuntimeDump       `json:"orders,omitempty"`
	Queue     *queueRuntimeDump       `json:"queue,omitempty"`
	Terrain   *terrainRuntimeDump     `json:"terrain,omitempty"`
	VFXLights *vfxLightsRuntimeDump   `json:"vfxLights,omitempty"`
	Voices    *voiceBattleRuntimeDump `json:"voices,omitempty"`
	Atlas     *atlasRuntimeDump       `json:"atlas,omitempty"`
	TeamColor *teamColorRuntimeDump   `json:"teamColor,omitempty"`
	OK        bool                    `json:"ok"`
}

type audioInitRuntimeDump struct {
	OK                    bool                 `json:"ok"`
	Mode                  string               `json:"mode"`
	RequestedBackend      string               `json:"requestedBackend"`
	DeviceError           string               `json:"deviceError,omitempty"`
	Backend               string               `json:"backend"`
	BackendSources        int                  `json:"backendSources"`
	AccountingMaxVoices   int                  `json:"accountingMaxVoices"`
	CameraFocus           litaudio.Vec3        `json:"cameraFocus"`
	CameraEye             litaudio.Vec3        `json:"cameraEye"`
	Listener              litaudio.Vec3        `json:"listener"`
	ListenerMatchesFocus  bool                 `json:"listenerMatchesFocus"`
	ListenerMatchesEye    bool                 `json:"listenerMatchesEye"`
	Snapshot              litaudio.Snapshot    `json:"snapshot"`
	NullAccountingHash    string               `json:"nullAccountingHash"`
	BackendAccountingHash string               `json:"backendAccountingHash"`
	AccountingMatchesNull bool                 `json:"accountingMatchesNull"`
	PanTrace              []audioPanTraceDump  `json:"panTrace"`
	PanSignFlipped        bool                 `json:"panSignFlipped"`
	SimHash               audioSimHashPairDump `json:"simHash"`
	Errors                []string             `json:"errors,omitempty"`
}

type audioPanTraceDump struct {
	Step     string        `json:"step"`
	Listener litaudio.Vec3 `json:"listener"`
	Emitter  litaudio.Vec3 `json:"emitter"`
	Pan      float64       `json:"pan"`
	Gain     float64       `json:"gain"`
}

type audioSimHashPairDump struct {
	AudioOff   string `json:"audioOff"`
	AudioOn    string `json:"audioOn"`
	AudioCalls int    `json:"audioCalls"`
	Equal      bool   `json:"equal"`
}

type audioAccountingState struct {
	Listener   litaudio.Vec3    `json:"listener"`
	VoiceCount int              `json:"voiceCount"`
	MaxVoices  int              `json:"maxVoices"`
	Culled     int              `json:"culled"`
	Dropped    int              `json:"dropped"`
	Voices     []litaudio.Voice `json:"voices"`
	ChannelVol []float64        `json:"channelVol"`
	GroupVol   []float64        `json:"groupVol"`
}

type vfxLightsRuntimeDump struct {
	Scene       string                  `json:"scene"`
	LowPreset   bool                    `json:"lowPreset"`
	MaxActive   int                     `json:"maxActive"`
	FinalActive int                     `json:"finalActive"`
	Events      []vfxLightEventDump     `json:"events"`
	Slots       []litrender.VFXSlotInfo `json:"slots"`
	OK          bool                    `json:"ok"`
	Errors      []string                `json:"errors,omitempty"`
}

type vfxLightEventDump struct {
	Request  string                `json:"request"`
	Priority litrender.VFXPriority `json:"priority"`
	Decision litrender.VFXDecision `json:"decision"`
}

type atlasRuntimeDump struct {
	Scene               string                            `json:"scene"`
	Preset              litasset.AtlasPreset              `json:"preset"`
	Source              atlasSourceDump                   `json:"source"`
	Upload              litasset.AtlasUpload              `json:"upload"`
	Material            litrender.AtlasMaterialSnapshot   `json:"material"`
	AdditionalMaterials []litrender.AtlasMaterialSnapshot `json:"additionalMaterials,omitempty"`
	MaterialInstances   int                               `json:"materialInstances"`
	SampledSwatches     []atlasSwatchDump                 `json:"sampledSwatches"`
	RuntimeSwitch       []litrender.AtlasMaterialSnapshot `json:"runtimeSwitch,omitempty"`
	RuntimeSwitchReused *bool                             `json:"runtimeSwitchReused,omitempty"`
	OK                  bool                              `json:"ok"`
	Errors              []string                          `json:"errors,omitempty"`
}

type atlasSourceDump struct {
	Name   string `json:"name"`
	Width  int    `json:"width"`
	Height int    `json:"height"`
	SHA256 string `json:"sha256"`
}

type atlasSwatchDump struct {
	Label string `json:"label"`
	X     int    `json:"x"`
	Y     int    `json:"y"`
	R     uint8  `json:"r"`
	G     uint8  `json:"g"`
	B     uint8  `json:"b"`
	A     uint8  `json:"a"`
}

type teamColorRuntimeDump struct {
	Scene             string               `json:"scene"`
	Preset            litasset.AtlasPreset `json:"preset"`
	MaterialPath      string               `json:"materialPath"`
	Uniforms          []string             `json:"uniforms"`
	Upload            litasset.AtlasUpload `json:"upload"`
	MaterialInstances int                  `json:"materialInstances"`
	Units             []teamColorUnitDump  `json:"units"`
	FlashIndex        int                  `json:"flashIndex,omitempty"`
	FadeIndex         int                  `json:"fadeIndex,omitempty"`
	OK                bool                 `json:"ok"`
	Errors            []string             `json:"errors,omitempty"`
}

type teamColorUnitDump struct {
	Index int                      `json:"index"`
	X     float32                  `json:"x"`
	Z     float32                  `json:"z"`
	State litrender.TeamColorState `json:"state"`
}

type mapDataRuntimeDump struct {
	OK              bool                       `json:"ok"`
	Path            string                     `json:"path"`
	Width           int                        `json:"width"`
	Height          int                        `json:"height"`
	PathingWidth    int                        `json:"pathingWidth"`
	PathingHeight   int                        `json:"pathingHeight"`
	Biome           string                     `json:"biome"`
	Fingerprint     string                     `json:"fingerprint"`
	Counts          mapDataCounts              `json:"counts"`
	Starts          []litmapdata.StartLocation `json:"starts"`
	Doodads         []litmapdata.Doodad        `json:"doodads"`
	PathingSamples  []mapDataPathingSample     `json:"pathingSamples"`
	HeightSamples   []mapDataHeightSample      `json:"heightSamples"`
	SplatSamples    []mapDataSplatSample       `json:"splatSamples"`
	HandCheckSource []string                   `json:"handCheckSource"`
}

type mapDataCounts struct {
	PathingCells int `json:"pathingCells"`
	Walkable     int `json:"walkable"`
	Buildable    int `json:"buildable"`
	Water        int `json:"water"`
	Ramps        int `json:"ramps"`
	HeightVerts  int `json:"heightVerts"`
	SplatCells   int `json:"splatCells"`
	Doodads      int `json:"doodads"`
}

type mapDataPathingSample struct {
	X         int              `json:"x"`
	Y         int              `json:"y"`
	Flags     uint8            `json:"flags"`
	Walkable  bool             `json:"walkable"`
	Buildable bool             `json:"buildable"`
	Water     bool             `json:"water"`
	Cliff     litmapdata.Cliff `json:"cliff"`
	CliffText string           `json:"cliffText"`
}

type mapDataHeightSample struct {
	X      int   `json:"x"`
	Y      int   `json:"y"`
	Height int32 `json:"height"`
}

type mapDataSplatSample struct {
	X      int                    `json:"x"`
	Y      int                    `json:"y"`
	Weight litmapdata.SplatWeight `json:"weight"`
}

type terrainRuntimeDump struct {
	Scene             string                    `json:"scene"`
	MapPath           string                    `json:"mapPath"`
	Wireframe         bool                      `json:"wireframe"`
	VertexCount       int                       `json:"vertexCount"`
	TriangleCount     int                       `json:"triangleCount"`
	InvertedTriangles int                       `json:"invertedTriangles"`
	MaxHeightDiff     int32                     `json:"maxHeightDiff"`
	HeightSamples     []litterrain.HeightSample `json:"heightSamples"`
	BorderVertices    []terrainBorderDump       `json:"borderVertices"`
	Units             []terrainUnitDump         `json:"units,omitempty"`

	// Chunk fields are populated only for the terrain-chunks scene.
	Chunked        bool  `json:"chunked"`
	ChunkCells     int   `json:"chunkCells,omitempty"`
	ChunkCount     int   `json:"chunkCount,omitempty"`
	ChunkCols      int   `json:"chunkCols,omitempty"`
	ChunkRows      int   `json:"chunkRows,omitempty"`
	MaxChunkTris   int   `json:"maxChunkTris,omitempty"`
	ChunkTris      []int `json:"chunkTris,omitempty"`
	SeamMismatches int   `json:"seamMismatches"`

	OK     bool     `json:"ok"`
	Errors []string `json:"errors,omitempty"`
}

type terrainBorderDump struct {
	X      int     `json:"x"`
	Y      int     `json:"y"`
	WorldX float32 `json:"worldX"`
	WorldY float32 `json:"worldY"`
	WorldZ float32 `json:"worldZ"`
}

type terrainUnitDump struct {
	Name    string  `json:"name"`
	VertexX int     `json:"vertexX"`
	VertexY int     `json:"vertexY"`
	WorldX  float32 `json:"worldX"`
	GroundY float32 `json:"groundY"`
	WorldZ  float32 `json:"worldZ"`
}

type resolutionFlag struct {
	W, H int
	set  bool
}

func (r *resolutionFlag) String() string {
	if r == nil || r.W == 0 || r.H == 0 {
		return ""
	}
	return fmt.Sprintf("%dx%d", r.W, r.H)
}

func (r *resolutionFlag) Set(s string) error {
	before := *r
	widthText, heightText, ok := strings.Cut(s, "x")
	if !ok || widthText == "" || heightText == "" || strings.Contains(heightText, "x") {
		return fmt.Errorf("resolution must be WIDTHxHEIGHT, got %q", s)
	}
	w, werr := strconv.Atoi(widthText)
	h, herr := strconv.Atoi(heightText)
	if werr != nil || herr != nil || w <= 0 || h <= 0 {
		*r = before
		return fmt.Errorf("resolution must be WIDTHxHEIGHT, got %q", s)
	}
	r.W, r.H, r.set = w, h, true
	return nil
}

type canvasRegion struct {
	name   string
	anchor lithud.Anchor
	ref    lithud.RefRect
	color  math32.Color4
}

type canvasRegionDump struct {
	Name   string         `json:"name"`
	Anchor string         `json:"anchor"`
	Kind   string         `json:"kind,omitempty"`
	Parent string         `json:"parent,omitempty"`
	Atlas  string         `json:"atlas,omitempty"`
	CellsX int            `json:"cellsX,omitempty"`
	CellsY int            `json:"cellsY,omitempty"`
	Ref    lithud.RefRect `json:"ref"`
	Rect   lithud.Rect    `json:"rect"`
}

type canvasSnapshot struct {
	Width   int                `json:"width"`
	Height  int                `json:"height"`
	UIScale float64            `json:"uiScale"`
	Scale   float64            `json:"scale"`
	Rects   []canvasRegionDump `json:"rects"`
}

type canvasDump struct {
	Mode         string                   `json:"mode"`
	Before       *canvasSnapshot          `json:"before,omitempty"`
	After        canvasSnapshot           `json:"after"`
	HUD          hudRuntimeDump           `json:"hud,omitempty"`
	CommandCard  *commandCardRuntimeDump  `json:"commandCard,omitempty"`
	ResourceBar  *resourceBarRuntimeDump  `json:"resourceBar,omitempty"`
	CampaignMenu *campaignMenuRuntimeDump `json:"campaignMenu,omitempty"`
	OK           bool                     `json:"ok"`
	Errors       []string                 `json:"errors,omitempty"`
}

type hudRuntimeDump struct {
	AtlasPath              string              `json:"atlasPath"`
	Locale                 string              `json:"locale"`
	WidgetPanels           int                 `json:"widgetPanels"`
	Labels                 int                 `json:"labels"`
	ExpectedGUIDrawCalls   int                 `json:"expectedGuiDrawCalls"`
	DrawCallBudget         int                 `json:"drawCallBudget"`
	ActualGUIDrawCalls     int                 `json:"actualGuiDrawCalls"`
	GUIStateChanges        int                 `json:"guiStateChanges"`
	WorstUpdateMicrosFrame float64             `json:"worstUpdateMicrosPerFrame"`
	UpdateScenarios        lithud.FSVScenarios `json:"updateScenarios"`
}

type commandCardRuntimeDump struct {
	TablePath     string                    `json:"tablePath"`
	KeymapPath    string                    `json:"keymapPath"`
	KeymapProfile string                    `json:"keymapProfile"`
	Scenario      string                    `json:"scenario"`
	Current       commandCardCaseDump       `json:"current"`
	Cases         []commandCardCaseDump     `json:"cases"`
	Clicks        []lithud.CommandCardClick `json:"clicks"`
	KeyPresses    []commandCardKeyPressDump `json:"keyPresses,omitempty"`
	Emitted       []commandRecordDump       `json:"emitted"`
}

type commandCardCaseDump struct {
	Name           string                        `json:"name"`
	Selection      string                        `json:"selection"`
	ActiveSubgroup string                        `json:"activeSubgroup,omitempty"`
	Visible        bool                          `json:"visible"`
	Summary        string                        `json:"summary"`
	Update         lithud.CommandCardUpdate      `json:"update"`
	Slots          []lithud.CommandCardSlotState `json:"slots"`
}

type commandCardKeyPressDump struct {
	Key           string             `json:"key"`
	Action        string             `json:"action,omitempty"`
	Slot          uint8              `json:"slot,omitempty"`
	Accepted      bool               `json:"accepted"`
	PendingTarget bool               `json:"pendingTarget,omitempty"`
	Reason        string             `json:"reason,omitempty"`
	Emitted       *commandRecordDump `json:"emitted,omitempty"`
}

type commandRecordDump struct {
	Version   uint8    `json:"version"`
	Player    uint8    `json:"player"`
	Seq       uint16   `json:"seq"`
	Opcode    uint8    `json:"opcode"`
	Flags     uint8    `json:"flags"`
	UnitCount uint8    `json:"unitCount"`
	Units     []uint32 `json:"units"`
	Target    uint32   `json:"target,omitempty"`
	PointX    int64    `json:"pointX"`
	PointY    int64    `json:"pointY"`
	Data      uint16   `json:"data,omitempty"`
}

type selectionRuntimeDump struct {
	Scenario string              `json:"scenario"`
	Current  selectionCaseDump   `json:"current"`
	Cases    []selectionCaseDump `json:"cases"`
	OK       bool                `json:"ok"`
	Errors   []string            `json:"errors,omitempty"`
}

type selectionCaseDump struct {
	Name                  string        `json:"name"`
	Gesture               string        `json:"gesture"`
	Marquee               litinput.Rect `json:"marquee,omitempty"`
	ClickX                float32       `json:"clickX,omitempty"`
	ClickY                float32       `json:"clickY,omitempty"`
	Selection             []uint32      `json:"selection"`
	Expected              []uint32      `json:"expected"`
	ActiveSubgroup        uint8         `json:"activeSubgroup,omitempty"`
	ActiveSubgroupTypeID  uint16        `json:"activeSubgroupTypeID,omitempty"`
	Candidates            uint16        `json:"candidates,omitempty"`
	NormalPriority        uint16        `json:"normalPriority,omitempty"`
	CommandRecordsEmitted uint16        `json:"commandRecordsEmitted"`
	OK                    bool          `json:"ok"`
}

type groupRuntimeDump struct {
	Current groupCaseDump   `json:"current"`
	Cases   []groupCaseDump `json:"cases"`
	OK      bool            `json:"ok"`
	Errors  []string        `json:"errors,omitempty"`
}

type groupCaseDump struct {
	Name                  string   `json:"name"`
	Gesture               string   `json:"gesture"`
	Group                 uint8    `json:"group"`
	GroupIDs              []uint32 `json:"groupIDs"`
	ExpectedGroupIDs      []uint32 `json:"expectedGroupIDs"`
	Selection             []uint32 `json:"selection"`
	Expected              []uint32 `json:"expected"`
	Pruned                uint8    `json:"pruned,omitempty"`
	ExpectedPruned        uint8    `json:"expectedPruned,omitempty"`
	CenterRequested       bool     `json:"centerRequested"`
	ExpectedCenter        bool     `json:"expectedCenter"`
	CenterX               float32  `json:"centerX,omitempty"`
	CenterZ               float32  `json:"centerZ,omitempty"`
	ExpectedCenterX       float32  `json:"expectedCenterX,omitempty"`
	ExpectedCenterZ       float32  `json:"expectedCenterZ,omitempty"`
	OldID                 uint32   `json:"oldID,omitempty"`
	RecycledID            uint32   `json:"recycledID,omitempty"`
	CommandRecordsEmitted uint16   `json:"commandRecordsEmitted"`
	OK                    bool     `json:"ok"`
}

type orderRuntimeDump struct {
	Scenario string          `json:"scenario"`
	Current  orderCaseDump   `json:"current"`
	Cases    []orderCaseDump `json:"cases"`
	OK       bool            `json:"ok"`
	Errors   []string        `json:"errors,omitempty"`
}

type orderCaseDump struct {
	Name            string              `json:"name"`
	Gesture         string              `json:"gesture"`
	TargetClass     string              `json:"targetClass"`
	Selection       []uint32            `json:"selection"`
	Target          uint32              `json:"target,omitempty"`
	Feedback        string              `json:"feedback"`
	EncodedBytes    int                 `json:"encodedBytes"`
	Records         []commandRecordDump `json:"records"`
	ExpectedOpcodes []uint32            `json:"expectedOpcodes"`
	OK              bool                `json:"ok"`
}

type queueRuntimeDump struct {
	Scenario        string              `json:"scenario"`
	Unit            uint32              `json:"unit"`
	InitialSequence []queuePointDump    `json:"initialSequence"`
	SecondSequence  []queuePointDump    `json:"secondSequence"`
	Records         []commandRecordDump `json:"records"`
	QueuedFlagHex   string              `json:"queuedFlagHex"`
	QueuedFlagByte  uint8               `json:"queuedFlagByte"`
	Trace           []queueTraceDump    `json:"trace"`
	ScreenshotState queueTraceDump      `json:"screenshotState"`
	FinalState      queueTraceDump      `json:"finalState"`
	Replay          queueReplayDump     `json:"replay"`
	Cases           []queueCaseDump     `json:"cases"`
	OK              bool                `json:"ok"`
	Errors          []string            `json:"errors,omitempty"`
}

type queueTraceDump struct {
	Label          string           `json:"label"`
	Tick           uint32           `json:"tick"`
	Alive          bool             `json:"alive"`
	HasOrder       bool             `json:"hasOrder"`
	Pos            queuePointDump   `json:"pos"`
	MoveState      uint8            `json:"moveState"`
	Current        queueOrderDump   `json:"current"`
	Queue          []queueOrderDump `json:"queue"`
	QueueDepth     int              `json:"queueDepth"`
	TotalOrders    int              `json:"totalOrders"`
	OrderPoolFree  int32            `json:"orderPoolFree"`
	PathQueueDepth int              `json:"pathQueueDepth"`
	PathExpansions int32            `json:"pathExpansions"`
	Hash           string           `json:"hash"`
}

type queueOrderDump struct {
	Kind   uint8          `json:"kind"`
	Target uint32         `json:"target,omitempty"`
	Point  queuePointDump `json:"point"`
	Data   uint16         `json:"data,omitempty"`
}

type queuePointDump struct {
	XRaw int64 `json:"xRaw"`
	YRaw int64 `json:"yRaw"`
	X    int64 `json:"x"`
	Y    int64 `json:"y"`
}

type queueReplayDump struct {
	FirstHash        string         `json:"firstHash"`
	SecondHash       string         `json:"secondHash"`
	Equal            bool           `json:"equal"`
	FirstFinalPos    queuePointDump `json:"firstFinalPos"`
	SecondFinalPos   queuePointDump `json:"secondFinalPos"`
	CollapseAtTick   uint32         `json:"collapseAtTick"`
	CommandsReplayed int            `json:"commandsReplayed"`
}

type queueCaseDump struct {
	Name         string           `json:"name"`
	Before       queueTraceDump   `json:"before"`
	After        queueTraceDump   `json:"after"`
	Drops        []queueEventDump `json:"drops,omitempty"`
	Expected     string           `json:"expected"`
	OK           bool             `json:"ok"`
	Error        string           `json:"error,omitempty"`
	PoolFreeBase int32            `json:"poolFreeBase,omitempty"`
}

type queueEventDump struct {
	Tick uint32 `json:"tick"`
	Kind uint16 `json:"kind"`
	Src  uint32 `json:"src"`
	Arg  int64  `json:"arg"`
}

type resourceBarRuntimeDump struct {
	Scenario string                    `json:"scenario"`
	Current  resourceBarCaseDump       `json:"current"`
	Cases    []resourceBarCaseDump     `json:"cases"`
	Feedback []lithud.ResourceFeedback `json:"feedback,omitempty"`
}

type resourceBarCaseDump struct {
	Name      string                    `json:"name"`
	Sim       resourceBarValues         `json:"sim"`
	Displayed string                    `json:"displayed"`
	Update    lithud.ResourceBarUpdate  `json:"update"`
	Feedback  []lithud.ResourceFeedback `json:"feedback,omitempty"`
}

type resourceBarValues struct {
	Gold     int `json:"gold"`
	Lumber   int `json:"lumber"`
	FoodUsed int `json:"foodUsed"`
	FoodCap  int `json:"foodCap"`
	Upkeep   int `json:"upkeep"`
}

type campaignMenuRuntimeDump struct {
	Scenario       string                     `json:"scenario"`
	Locale         string                     `json:"locale"`
	Screen         lithud.CampaignMenuScreen  `json:"screen"`
	Layout         lithud.CampaignMenuLayout  `json:"layout"`
	Catalog        *litcampaign.CatalogView   `json:"catalog,omitempty"`
	View           *litcampaign.View          `json:"view,omitempty"`
	BeforeStore    *litcampaign.StoreSnapshot `json:"beforeStore,omitempty"`
	AfterStore     litcampaign.StoreSnapshot  `json:"afterStore"`
	Checkpoint     string                     `json:"checkpoint,omitempty"`
	CheckpointRead bool                       `json:"checkpointRead,omitempty"`
	OK             bool                       `json:"ok"`
	Errors         []string                   `json:"errors,omitempty"`
}

func main() {
	res := resolutionFlag{W: defaultWidth, H: defaultHeight}
	resizeFrom := resolutionFlag{}
	sceneName := flag.String("scene", "counted", "scene to render: empty, single, counted, culled, shared, twomats, transparent, camera-rig, atlas, atlas-two, teamcolors, teamcolors-one, teamcolors-flash, teamcolors-fade, terrain, terrain-units, terrain-chunks, spellstorm, battle500, basecamp, campaign-menu")
	presetText := flag.String("preset", "high", "atlas texture preset: high, medium, or low")
	dumpMapPath := flag.String("dump-map", "", "load map data directory and print decoded terrain JSON, e.g. data/maps/test64")
	dumpAudioPath := flag.String("dump-audio", "", "load an audio asset directory and print decoded/resident/streamed JSON")
	dumpAudioInitMode := flag.String("dump-audio-init", "", "print audio init/accounting JSON for backend mode: null, openal, or auto")
	shotPath := flag.String("shot", "artifacts/stats-hud.png", "screenshot output path")
	dumpPath := flag.String("dump", "artifacts/stats.json", "stats JSON output path")
	autotest := flag.Bool("autotest", false, "exit non-zero if dumped counters do not match the hand count")
	autotestSelect := flag.Bool("autotest-select", false, "render the drag-select input FSV fixture")
	autotestGroups := flag.Bool("autotest-groups", false, "render the control-group input FSV fixture")
	autotestOrders := flag.Bool("autotest-orders", false, "render the smart-right-click order FSV fixture")
	autotestQueue := flag.Bool("autotest-queue", false, "render the shift-queue order FSV fixture")
	autotestAudio := flag.Bool("autotest-audio", false, "run the basecamp audio domain FSV fixture and dump gain/pan/cull JSON")
	wireframe := flag.Bool("wireframe", false, "render terrain scene material as wireframe")
	debugFarplane := flag.Float64("debug-farplane", 1, "multiply the computed far plane by this factor (#40 invariant probe: 2x must not change the visible-graphic set)")
	vfxLowPreset := flag.Bool("vfx-low-preset", false, "spellstorm scene: run the VFX light pool in low preset (requests accounted, no light bound)")
	hudMode := flag.Bool("hud", false, "render the HUD virtual-canvas FSV fixture")
	cameraMode := flag.String("camera", "persp", "RTS camera projection: persp or ortho")
	zoomMode := flag.String("zoom", "default", "RTS camera zoom request: default, min, max, below-min, above-max, or a numeric world-unit distance")
	localeTag := flag.String("locale", "en", "locale tag for HUD strings when -hud is set")
	cardScenario := flag.String("card-scenario", "", "command-card FSV scenario for -hud -scene basecamp: unit, building, subgroup, enemy, cooldown, empty")
	resbarScenario := flag.String("resbar-scenario", "", "resource-bar FSV scenario for -hud -scene basecamp: initial, after-spend, foodcap, insufficient, large")
	campaignScenario := flag.String("campaign-scenario", "", "campaign-menu FSV scenario for -hud -scene campaign-menu: campaign-select, fresh, unlocked, save-load, missing-archive")
	selectScenario := flag.String("select-scenario", "mixed", "selection FSV scenario for -autotest-select: mixed, cap, typesel")
	keymapPath := flag.String("keymap", "", "optional TOML keymap override for HUD command-card hotkeys")
	uiScale := flag.Float64("uiscale", 1, "HUD user UI scale multiplier; clamped to [0.75,1.5]")
	flag.Var(&res, "res", "window resolution WIDTHxHEIGHT")
	flag.Var(&resizeFrom, "resize-from", "optional pre-resize WIDTHxHEIGHT to include in HUD canvas dump")
	flag.Parse()
	atlasPreset, err := litasset.ParseAtlasPreset(*presetText)
	if err != nil {
		fmt.Fprintf(os.Stderr, "renderdemo: preset: %v\n", err)
		os.Exit(1)
	}
	if strings.TrimSpace(*dumpMapPath) != "" {
		dump, err := buildMapDataDump(*dumpMapPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: dump-map: %v\n", err)
			os.Exit(1)
		}
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		if err := enc.Encode(dump); err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: dump-map: %v\n", err)
			os.Exit(1)
		}
		return
	}
	if strings.TrimSpace(*dumpAudioPath) != "" {
		dump, err := buildAudioLoadDump(*dumpAudioPath)
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		if encErr := enc.Encode(dump); encErr != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: dump-audio: %v\n", encErr)
			os.Exit(1)
		}
		if err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: dump-audio: %v\n", err)
			os.Exit(1)
		}
		return
	}
	if strings.TrimSpace(*dumpAudioInitMode) != "" {
		dump, err := buildAudioInitDump(*dumpAudioInitMode)
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		if encErr := enc.Encode(dump); encErr != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: dump-audio-init: %v\n", encErr)
			os.Exit(1)
		}
		if err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: dump-audio-init: %v\n", err)
			os.Exit(1)
		}
		if !dump.OK {
			fmt.Fprintf(os.Stderr, "renderdemo: dump-audio-init failed: %s\n", strings.Join(dump.Errors, "; "))
			os.Exit(1)
		}
		return
	}
	if *autotestAudio {
		dump, err := buildAudioDomainDump(*sceneName)
		if dump != nil && *dumpPath != "" {
			if writeErr := writeJSONFile(*dumpPath, dump); writeErr != nil {
				fmt.Fprintf(os.Stderr, "renderdemo: autotest-audio dump: %v\n", writeErr)
				os.Exit(1)
			}
		}
		if err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: autotest-audio: %v\n", err)
			os.Exit(1)
		}
		if dump == nil {
			fmt.Fprintf(os.Stderr, "renderdemo: autotest-audio: missing dump\n")
			os.Exit(1)
		}
		fmt.Printf("audio-domains: scene=%s ok=%v playbacks=%d culled=%d dump=%s\n",
			dump.Scene, dump.OK, len(dump.Playbacks), dump.Snapshot.Culled, *dumpPath)
		if !dump.OK {
			fmt.Fprintf(os.Stderr, "renderdemo: autotest-audio failed: %s\n", strings.Join(dump.Errors, "; "))
			os.Exit(2)
		}
		return
	}
	if *hudMode && !res.set {
		res = resolutionFlag{W: 1366, H: 768, set: true}
	}

	a := app.App(res.W, res.H, "LitD render stats demo")
	scene := core.NewNode()
	cameraRig, err := buildCamera(res.W, res.H, *zoomMode, *cameraMode)
	if err != nil {
		fmt.Fprintf(os.Stderr, "renderdemo: camera: %v\n", err)
		os.Exit(1)
	}
	cam := cameraRig.Camera
	if *debugFarplane != 1 {
		// #40 invariant probe: stretching the far plane must not pull anything new
		// into the visible-graphic set — the production far plane is not clipping
		// visible content.
		cam.SetFar(cam.Far() * float32(*debugFarplane))
	}
	fmt.Fprintf(os.Stderr, "renderdemo: clip planes near=%g far=%g (farplaneFactor=%g)\n", cam.Near(), cam.Far(), *debugFarplane)

	var spec sceneSpec
	var canvasFSV canvasDump
	var selectionFSV *selectionRuntimeDump
	var groupFSV *groupRuntimeDump
	var orderFSV *orderRuntimeDump
	var queueFSV *queueRuntimeDump
	var terrainFSV *terrainRuntimeDump
	var vfxFSV *vfxLightsRuntimeDump
	var voiceFSV *voiceBattleRuntimeDump
	var atlasFSV *atlasRuntimeDump
	var teamColorFSV *teamColorRuntimeDump
	if *hudMode {
		table, err := litlocale.Load(os.DirFS("data"), *localeTag)
		if err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: locale: %v\n", err)
			os.Exit(1)
		}
		canvasFSV, err = buildCanvasHUD(scene, res, *uiScale, resizeFrom, *sceneName, *cardScenario, *resbarScenario, *campaignScenario, *localeTag, *keymapPath, table, lithud.HUDStringsFromLocale(table))
		if err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: %v\n", err)
			os.Exit(1)
		}
	} else if *autotestSelect {
		buildLights(scene)
		var err error
		selectionFSV, err = buildSelectionFSV(scene, cameraRig, res, *selectScenario)
		if err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: selection: %v\n", err)
			os.Exit(1)
		}
		spec = sceneSpec{name: "select-" + selectionFSV.Scenario, expected: expectedStats(0, 0, 0, 0, 0, 0)}
		addStatsHUD(scene, spec)
	} else if *autotestGroups {
		buildLights(scene)
		groupFSV = buildGroupFSV(scene, cameraRig)
		spec = sceneSpec{name: "group-recall", expected: expectedStats(0, 0, 0, 0, 0, 0)}
		addStatsHUD(scene, spec)
	} else if *autotestOrders {
		if strings.ToLower(strings.TrimSpace(*sceneName)) != "economy" {
			fmt.Fprintf(os.Stderr, "renderdemo: orders fixture requires -scene economy\n")
			os.Exit(1)
		}
		buildLights(scene)
		var err error
		orderFSV, err = buildSmartOrderFSV(scene)
		if err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: orders: %v\n", err)
			os.Exit(1)
		}
		spec = sceneSpec{name: "orders-" + orderFSV.Scenario, expected: expectedStats(0, 0, 0, 0, 0, 0)}
		addStatsHUD(scene, spec)
	} else if *autotestQueue {
		if strings.ToLower(strings.TrimSpace(*sceneName)) != "moveorder" {
			fmt.Fprintf(os.Stderr, "renderdemo: queue fixture requires -scene moveorder\n")
			os.Exit(1)
		}
		buildLights(scene)
		var err error
		queueFSV, err = buildQueueFSV(scene)
		if err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: queue: %v\n", err)
			os.Exit(1)
		}
		spec = sceneSpec{name: "queue-" + queueFSV.Scenario, expected: expectedStats(0, 0, 0, 0, 0, 0)}
		addStatsHUD(scene, spec)
	} else {
		buildLights(scene)
		var err error
		if strings.HasPrefix(strings.ToLower(strings.TrimSpace(*sceneName)), "terrain") {
			spec, terrainFSV, err = buildTerrainFSV(scene, *sceneName, *wireframe)
			if err != nil {
				fmt.Fprintf(os.Stderr, "renderdemo: terrain: %v\n", err)
				os.Exit(1)
			}
		} else if strings.ToLower(strings.TrimSpace(*sceneName)) == "spellstorm" {
			spec, vfxFSV, err = buildSpellstormFSV(scene, *vfxLowPreset)
			if err != nil {
				fmt.Fprintf(os.Stderr, "renderdemo: spellstorm: %v\n", err)
				os.Exit(1)
			}
		} else if strings.ToLower(strings.TrimSpace(*sceneName)) == "battle500" {
			spec, voiceFSV, err = buildBattle500FSV(scene)
			if err != nil {
				fmt.Fprintf(os.Stderr, "renderdemo: battle500: %v\n", err)
				os.Exit(1)
			}
		} else if strings.ToLower(strings.TrimSpace(*sceneName)) == "atlas" {
			spec, atlasFSV, err = buildAtlasFSV(scene, atlasPreset)
			if err != nil {
				fmt.Fprintf(os.Stderr, "renderdemo: atlas: %v\n", err)
				os.Exit(1)
			}
		} else if strings.ToLower(strings.TrimSpace(*sceneName)) == "atlas-two" {
			spec, atlasFSV, err = buildAtlasTwoFSV(scene, atlasPreset)
			if err != nil {
				fmt.Fprintf(os.Stderr, "renderdemo: atlas-two: %v\n", err)
				os.Exit(1)
			}
		} else if strings.HasPrefix(strings.ToLower(strings.TrimSpace(*sceneName)), "teamcolors") {
			spec, teamColorFSV, err = buildTeamColorsFSV(scene, atlasPreset, strings.ToLower(strings.TrimSpace(*sceneName)))
			if err != nil {
				fmt.Fprintf(os.Stderr, "renderdemo: teamcolors: %v\n", err)
				os.Exit(1)
			}
		} else {
			spec, err = buildScene(scene, *sceneName)
			if err != nil {
				fmt.Fprintf(os.Stderr, "renderdemo: %v\n", err)
				os.Exit(1)
			}
		}
		addStatsHUD(scene, spec)
	}

	a.Subscribe(window.OnWindowSize, func(string, interface{}) {
		w, h := a.GetSize()
		a.Gls().Viewport(0, 0, int32(w), int32(h))
		cameraRig.SetAspect(float32(w) / float32(h))
	})
	a.Gls().Viewport(0, 0, int32(res.W), int32(res.H))
	a.Gls().ClearColor(0.03, 0.04, 0.05, 1)

	a.Run(func(rend *renderer.Renderer, _ time.Duration) {
		a.Gls().Clear(gls.DEPTH_BUFFER_BIT | gls.STENCIL_BUFFER_BIT | gls.COLOR_BUFFER_BIT)
		if err := rend.Render(scene, cam); err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: render: %v\n", err)
			os.Exit(1)
		}
		stats := litrender.ReadFrameStats(rend)
		if *hudMode {
			canvasFSV.recordFrameStats(stats)
		}
		var sceneDump renderDemoDump
		if !*hudMode {
			cameraDump := cameraRig.DumpWithLockProbeForViewport(91, 12, 45, res.W, res.H)
			pass := cameraDump.OK
			if selectionFSV != nil {
				pass = pass && selectionFSV.OK
			} else if groupFSV != nil {
				pass = pass && groupFSV.OK
			} else if orderFSV != nil {
				pass = pass && orderFSV.OK
			} else if queueFSV != nil {
				pass = pass && queueFSV.OK
			} else if terrainFSV != nil {
				pass = pass && terrainFSV.OK
			} else if vfxFSV != nil {
				pass = pass && vfxFSV.OK
			} else if voiceFSV != nil {
				pass = pass && voiceFSV.OK && stats == spec.expected
			} else if atlasFSV != nil {
				pass = pass && atlasFSV.OK && stats == spec.expected
			} else if teamColorFSV != nil {
				pass = pass && teamColorFSV.OK && stats == spec.expected
			} else {
				pass = pass && stats == spec.expected
			}
			sceneDump = renderDemoDump{FrameStats: stats, Scene: spec.name, Camera: cameraDump, Selection: selectionFSV, Groups: groupFSV, Orders: orderFSV, Queue: queueFSV, Terrain: terrainFSV, VFXLights: vfxFSV, Voices: voiceFSV, Atlas: atlasFSV, TeamColor: teamColorFSV, OK: pass}
		}
		if *shotPath != "" {
			if err := screenshot(a, *shotPath); err != nil {
				fmt.Fprintf(os.Stderr, "renderdemo: screenshot: %v\n", err)
				os.Exit(1)
			}
		}
		if *dumpPath != "" {
			if *hudMode {
				if err := writeJSONFile(*dumpPath, canvasFSV); err != nil {
					fmt.Fprintf(os.Stderr, "renderdemo: dump: %v\n", err)
					os.Exit(1)
				}
			} else {
				if err := writeJSONFile(*dumpPath, sceneDump); err != nil {
					fmt.Fprintf(os.Stderr, "renderdemo: dump: %v\n", err)
					os.Exit(1)
				}
			}
		}

		if *hudMode {
			out, _ := json.Marshal(canvasFSV)
			fmt.Printf("canvas: %s shot=%s dump=%s\n", out, *shotPath, *dumpPath)
			if *autotest && !canvasFSV.OK {
				os.Exit(2)
			}
			os.Exit(0)
		}

		actualJSON, _ := json.Marshal(stats)
		expectedJSON, _ := json.Marshal(spec.expected)
		fmt.Printf("stats: scene=%s actual=%s expected=%s pass=%v shot=%s dump=%s\n",
			spec.name, actualJSON, expectedJSON, sceneDump.OK, *shotPath, *dumpPath)
		if *autotest && !sceneDump.OK {
			os.Exit(2)
		}
		os.Exit(0)
	})
}

func buildMapDataDump(dir string) (mapDataRuntimeDump, error) {
	m, err := litmapdata.Load(os.DirFS("."), dir)
	if err != nil {
		return mapDataRuntimeDump{}, err
	}
	dump := mapDataRuntimeDump{
		OK:              true,
		Path:            m.Path,
		Width:           m.Width,
		Height:          m.Height,
		PathingWidth:    m.PathingWidth,
		PathingHeight:   m.PathingHeight,
		Biome:           m.Biome,
		Fingerprint:     fmt.Sprintf("%016x", m.Fingerprint),
		Starts:          m.Starts(),
		Doodads:         m.Doodads(),
		HandCheckSource: []string{"terrain.toml", "pathing.txt", "cliff.txt", "height.txt", "splat.txt", "doodads.toml"},
	}
	dump.Counts = mapDataCounts{
		PathingCells: m.PathingWidth * m.PathingHeight,
		HeightVerts:  (m.Width + 1) * (m.Height + 1),
		SplatCells:   m.Width * m.Height,
		Doodads:      len(dump.Doodads),
	}
	for y := 0; y < m.PathingHeight; y++ {
		for x := 0; x < m.PathingWidth; x++ {
			flags, _ := m.PathingAt(x, y)
			if flags&litmapdata.PathWalkable != 0 {
				dump.Counts.Walkable++
			}
			if flags&litmapdata.PathBuildable != 0 {
				dump.Counts.Buildable++
			}
			if flags&litmapdata.PathWater != 0 {
				dump.Counts.Water++
			}
			cliff, _ := m.CliffAt(x, y)
			if cliff.Ramp {
				dump.Counts.Ramps++
			}
		}
	}
	for _, p := range [][2]int{{10, 10}, {64, 124}, {126, 128}, {130, 128}, {224, 224}} {
		if sample, ok := mapDataPathingSampleAt(m, p[0], p[1]); ok {
			dump.PathingSamples = append(dump.PathingSamples, sample)
		}
	}
	for _, p := range [][2]int{{31, 4}, {32, 4}, {33, 4}, {64, 64}} {
		if h, ok := m.HeightAtVertex(p[0], p[1]); ok {
			dump.HeightSamples = append(dump.HeightSamples, mapDataHeightSample{X: p[0], Y: p[1], Height: h})
		}
	}
	for _, p := range [][2]int{{0, 0}, {16, 31}, {63, 63}} {
		if s, ok := m.SplatAt(p[0], p[1]); ok {
			dump.SplatSamples = append(dump.SplatSamples, mapDataSplatSample{X: p[0], Y: p[1], Weight: s})
		}
	}
	return dump, nil
}

func buildAudioLoadDump(dir string) (litaudio.LoadDump, error) {
	return litaudio.LoadRuntimeAssetsDir(dir)
}

func buildAudioInitDump(mode string) (audioInitRuntimeDump, error) {
	mode = strings.ToLower(strings.TrimSpace(mode))
	if mode == "" {
		mode = "auto"
	}
	dump := audioInitRuntimeDump{Mode: mode, RequestedBackend: mode}

	var backend litaudio.Backend
	switch mode {
	case "null":
	case "openal":
		b, err := openAudioBackendForDemo()
		if err != nil {
			dump.Errors = append(dump.Errors, err.Error())
			return dump, err
		}
		backend = b
	case "auto":
		b, err := openAudioBackendForDemo()
		if err != nil {
			dump.DeviceError = err.Error()
		} else {
			backend = b
		}
	default:
		err := fmt.Errorf("unknown audio init backend mode %q (want null, openal, or auto)", mode)
		dump.Errors = append(dump.Errors, err.Error())
		return dump, err
	}

	m := litaudio.NewManager(backend)
	focus, eye, err := audioInitCameraFocus()
	if err != nil {
		dump.Errors = append(dump.Errors, err.Error())
		_ = m.Close()
		return dump, err
	}
	applyAudioInitSequence(m, focus)
	snap := m.Dump()
	closeErr := m.Close()
	if closeErr != nil {
		dump.Errors = append(dump.Errors, closeErr.Error())
	}

	nullSnap := audioInitNullSnapshot(focus)
	hashPair, hashErr := audioInitSimHashPair()
	if hashErr != nil {
		dump.Errors = append(dump.Errors, hashErr.Error())
	}
	panTrace := buildAudioPanTrace()
	panFlipped := len(panTrace) == 2 && panTrace[0].Pan > 0 && panTrace[1].Pan < 0

	nullHash := audioAccountingHash(nullSnap)
	backendHash := audioAccountingHash(snap)
	dump.Backend = snap.Backend
	dump.BackendSources = snap.BackendSources
	dump.AccountingMaxVoices = snap.MaxVoices
	dump.CameraFocus = focus
	dump.CameraEye = eye
	dump.Listener = snap.Listener
	dump.ListenerMatchesFocus = snap.Listener == focus
	dump.ListenerMatchesEye = snap.Listener == eye
	dump.Snapshot = snap
	dump.NullAccountingHash = nullHash
	dump.BackendAccountingHash = backendHash
	dump.AccountingMatchesNull = nullHash == backendHash
	dump.PanTrace = panTrace
	dump.PanSignFlipped = panFlipped
	dump.SimHash = hashPair

	if !dump.ListenerMatchesFocus {
		dump.Errors = append(dump.Errors, "listener does not match camera focus")
	}
	if dump.ListenerMatchesEye {
		dump.Errors = append(dump.Errors, "listener matched camera eye instead of focus")
	}
	if snap.MaxVoices != litaudio.MaxVoices {
		dump.Errors = append(dump.Errors, fmt.Sprintf("manager max voices=%d want %d", snap.MaxVoices, litaudio.MaxVoices))
	}
	if mode == "null" && snap.Backend != "null" {
		dump.Errors = append(dump.Errors, fmt.Sprintf("null mode selected backend %q", snap.Backend))
	}
	if mode == "openal" && snap.Backend != "openal" {
		dump.Errors = append(dump.Errors, fmt.Sprintf("openal mode selected backend %q", snap.Backend))
	}
	if snap.Backend == "openal" && snap.BackendSources != litaudio.MaxVoices {
		dump.Errors = append(dump.Errors, fmt.Sprintf("OpenAL source pool=%d want %d", snap.BackendSources, litaudio.MaxVoices))
	}
	if snap.Backend == "null" && snap.BackendSources != 0 {
		dump.Errors = append(dump.Errors, fmt.Sprintf("null backend sources=%d want 0", snap.BackendSources))
	}
	if !dump.AccountingMatchesNull {
		dump.Errors = append(dump.Errors, "backend accounting does not match null accounting for the same event sequence")
	}
	if !panFlipped {
		dump.Errors = append(dump.Errors, "pan trace did not flip sign")
	}
	if !hashPair.Equal {
		dump.Errors = append(dump.Errors, "audio-on and audio-off state hashes differ")
	}
	dump.OK = len(dump.Errors) == 0
	if hashErr != nil {
		return dump, hashErr
	}
	if closeErr != nil {
		return dump, closeErr
	}
	return dump, nil
}

func audioInitCameraFocus() (focus, eye litaudio.Vec3, err error) {
	rig, err := buildCamera(defaultWidth, defaultHeight, "default", "persp")
	if err != nil {
		return litaudio.Vec3{}, litaudio.Vec3{}, err
	}
	rig.SetAnchor(math32.Vector3{X: 120, Y: 8, Z: 80})
	s := rig.Snapshot()
	return renderGroundVecToAudio(s.Anchor), renderGroundVecToAudio(s.Eye), nil
}

func renderGroundVecToAudio(v litrender.Vec3Snapshot) litaudio.Vec3 {
	return litaudio.Vec3{X: float64(v.X), Y: float64(v.Z), Z: float64(v.Y)}
}

func applyAudioInitSequence(m *litaudio.Manager, focus litaudio.Vec3) {
	m.SetListener(focus)
	m.Handle(api.AudioEvent{
		Kind: api.AudioPlayAt, Cue: api.CueID("renderdemo/audio-init-world"), Volume: 1, Pitch: 1,
		HasPos: true, Pos: api.Vec2{X: focus.X + 300, Y: focus.Y}, Z: focus.Z, Channel: api.ChannelEffects,
	})
	m.Handle(api.AudioEvent{
		Kind: api.AudioPlay, Cue: api.CueID("renderdemo/audio-init-ui"), Volume: 0.5, Pitch: 1,
		Channel: api.ChannelUI,
	})
	m.Handle(api.AudioEvent{
		Kind: api.AudioPlayMusic, Cue: api.CueID("renderdemo/audio-init-music"), Volume: 0.8, Pitch: 1,
		Channel: api.ChannelMusic,
	})
}

func audioInitNullSnapshot(focus litaudio.Vec3) litaudio.Snapshot {
	m := litaudio.NewManager(nil)
	applyAudioInitSequence(m, focus)
	s := m.Dump()
	_ = m.Close()
	return s
}

func buildAudioPanTrace() []audioPanTraceDump {
	m := litaudio.NewManager(nil)
	emitter := litaudio.Vec3{X: 300}
	m.Handle(api.AudioEvent{
		Kind: api.AudioPlayAt, Cue: api.CueID("renderdemo/audio-pan-fixed"), Volume: 1, Pitch: 1,
		HasPos: true, Pos: api.Vec2{X: emitter.X, Y: emitter.Y}, Z: emitter.Z, Channel: api.ChannelEffects,
	})
	before := m.Dump()
	m.SetListener(litaudio.Vec3{X: 600})
	after := m.Dump()
	_ = m.Close()
	trace := make([]audioPanTraceDump, 0, 2)
	if len(before.Voices) > 0 {
		trace = append(trace, audioPanTraceDump{
			Step: "listener-left-of-emitter", Listener: before.Listener, Emitter: emitter,
			Pan: before.Voices[0].Pan, Gain: before.Voices[0].Gain,
		})
	}
	if len(after.Voices) > 0 {
		trace = append(trace, audioPanTraceDump{
			Step: "listener-right-of-emitter", Listener: after.Listener, Emitter: emitter,
			Pan: after.Voices[0].Pan, Gain: after.Voices[0].Gain,
		})
	}
	return trace
}

func audioInitSimHashPair() (audioSimHashPairDump, error) {
	off, _, err := audioInitStateHash(false)
	if err != nil {
		return audioSimHashPairDump{}, err
	}
	on, calls, err := audioInitStateHash(true)
	if err != nil {
		return audioSimHashPairDump{}, err
	}
	return audioSimHashPairDump{
		AudioOff:   fmt.Sprintf("%016x", off),
		AudioOn:    fmt.Sprintf("%016x", on),
		AudioCalls: calls,
		Equal:      off == on,
	}, nil
}

func audioInitStateHash(withSink bool) (uint64, int, error) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 8, Seed: 227})
	if err != nil {
		return 0, 0, err
	}
	calls := 0
	if withSink {
		m := litaudio.NewManager(nil)
		defer m.Close()
		g.OnAudio(func(ev api.AudioEvent) {
			calls++
			m.Handle(ev)
		})
	}
	snd := g.CreateSound("renderdemo/audio-init-statehash")
	snd.PlayAt(api.Vec2{X: litaudio.PanWidth, Y: 0}, 0)
	g.SetChannelVolume(api.ChannelUI, 0.3)
	return g.StateHash(), calls, nil
}

func audioAccountingHash(s litaudio.Snapshot) string {
	state := audioAccountingState{
		Listener:   s.Listener,
		VoiceCount: s.VoiceCount,
		MaxVoices:  s.MaxVoices,
		Culled:     s.Culled,
		Dropped:    s.Dropped,
		Voices:     s.Voices,
		ChannelVol: s.ChannelVol,
		GroupVol:   s.GroupVol,
	}
	body, _ := json.Marshal(state)
	sum := sha256.Sum256(body)
	return fmt.Sprintf("%x", sum[:])
}

func mapDataPathingSampleAt(m *litmapdata.Map, x, y int) (mapDataPathingSample, bool) {
	flags, ok := m.PathingAt(x, y)
	if !ok {
		return mapDataPathingSample{}, false
	}
	cliff, _ := m.CliffAt(x, y)
	return mapDataPathingSample{
		X:         x,
		Y:         y,
		Flags:     uint8(flags),
		Walkable:  flags&litmapdata.PathWalkable != 0,
		Buildable: flags&litmapdata.PathBuildable != 0,
		Water:     flags&litmapdata.PathWater != 0,
		Cliff:     cliff,
		CliffText: cliffText(cliff),
	}, true
}

func cliffText(c litmapdata.Cliff) string {
	if c.Ramp {
		return "r" + strconv.Itoa(int(c.Level))
	}
	return strconv.Itoa(int(c.Level))
}

func buildCamera(width, height int, zoomText, projectionText string) (*litrender.RTSCamera, error) {
	cfg := litrender.DefaultRTSCameraConfig(float32(width) / float32(height))
	zoom, err := cameraZoomRequest(zoomText, cfg)
	if err != nil {
		return nil, err
	}
	projection, err := litrender.ParseRTSCameraProjection(projectionText)
	if err != nil {
		return nil, err
	}
	rig := litrender.NewRTSCamera(cfg)
	rig.SetZoomRequested(zoom)
	if err := rig.SetProjectionMode(projection); err != nil {
		return nil, err
	}
	return rig, nil
}

func cameraZoomRequest(zoomText string, cfg litrender.RTSCameraConfig) (float32, error) {
	switch strings.ToLower(strings.TrimSpace(zoomText)) {
	case "", "default", "zdefault":
		return cfg.Zoom, nil
	case "min", "zmin":
		return cfg.ZoomMin, nil
	case "max", "zmax":
		return cfg.ZoomMax, nil
	case "below-min":
		return cfg.ZoomMin * 0.5, nil
	case "above-max":
		return cfg.ZoomMax * 2, nil
	default:
		value, err := strconv.ParseFloat(zoomText, 32)
		if err != nil {
			return 0, fmt.Errorf("unknown zoom request %q", zoomText)
		}
		return float32(value), nil
	}
}

func buildLights(scene *core.Node) {
	scene.Add(light.NewAmbient(&math32.Color{R: 1, G: 1, B: 1}, 0.55))
	sun := light.NewDirectional(&math32.Color{R: 1, G: 1, B: 1}, 0.75)
	sun.SetPosition(3, 8, 5)
	scene.Add(sun)
}

func buildScene(scene *core.Node, name string) (sceneSpec, error) {
	geom := geometry.NewBox(0.8, 0.8, 0.8)
	blue := material.NewStandard(&math32.Color{R: 0.20, G: 0.45, B: 0.95})
	red := material.NewStandard(&math32.Color{R: 0.95, G: 0.24, B: 0.18})

	switch name {
	case "empty":
		return sceneSpec{name: name, expected: expectedStats(0, 0, 0, 0, 0, 0)}, nil
	case "single":
		addMesh(scene, geom, blue, 0, 0, 0)
		return sceneSpec{name: name, expected: expectedStats(1, 0, 1, 0, 1, 0)}, nil
	case "counted":
		for i := -2; i <= 2; i++ {
			addMesh(scene, geom, blue, float32(i), 0, 0)
		}
		return sceneSpec{name: name, expected: expectedStats(5, 0, 5, 0, 1, 0)}, nil
	case "culled":
		addMesh(scene, geom, blue, 0, 0, 0)
		addMesh(scene, geom, blue, 100000, 0, 0)
		return sceneSpec{name: name, expected: expectedStats(1, 1, 1, 0, 1, 0)}, nil
	case "shared":
		addMesh(scene, geom, blue, -0.6, 0, 0)
		addMesh(scene, geom, blue, 0.6, 0, 0)
		return sceneSpec{name: name, expected: expectedStats(2, 0, 2, 0, 1, 0)}, nil
	case "twomats":
		addMesh(scene, geom, blue, -0.6, 0, 0)
		addMesh(scene, geom, red, 0.6, 0, 0)
		return sceneSpec{name: name, expected: expectedStats(2, 0, 2, 0, 2, 0)}, nil
	case "transparent":
		blue.SetTransparent(true)
		blue.SetOpacity(0.65)
		addMesh(scene, geom, blue, 0, 0, 0)
		return sceneSpec{name: name, expected: expectedStats(1, 0, 0, 1, 0, 1)}, nil
	case "camera-rig":
		addCameraRigScene(scene)
		return sceneSpec{name: name, expected: expectedStats(6, 0, 6, 0, 3, 0)}, nil
	default:
		return sceneSpec{}, fmt.Errorf("unknown scene %q", name)
	}
}

func buildAtlasFSV(scene *core.Node, preset litasset.AtlasPreset) (sceneSpec, *atlasRuntimeDump, error) {
	src, err := syntheticAtlasSource("synthetic/faction-vigil.atlas.png", atlasPaletteVigil())
	if err != nil {
		return sceneSpec{}, nil, err
	}
	cache := litrender.NewAtlasMaterialCache()
	entry, err := cache.Material(src, preset)
	if err != nil {
		return sceneSpec{}, nil, err
	}
	uploadImg, upload, err := litasset.BuildAtlasUpload(src, preset)
	if err != nil {
		return sceneSpec{}, nil, err
	}

	plane := graphic.NewMesh(geometry.NewPlane(1120, 1120), entry.Material)
	plane.SetRotationX(-math32.Pi / 2)
	plane.SetPosition(0, 8, 0)
	scene.Add(plane)

	dump := &atlasRuntimeDump{
		Scene:  "atlas",
		Preset: preset,
		Source: atlasSourceDump{
			Name:   src.Name,
			Width:  src.Width,
			Height: src.Height,
			SHA256: src.SHA256,
		},
		Upload:            entry.Upload,
		Material:          cache.Snapshot(entry),
		MaterialInstances: cache.Count(),
		OK:                true,
	}
	if upload.SHA256 != entry.Upload.SHA256 || upload.Width != entry.Upload.Width || upload.Height != entry.Upload.Height {
		dump.OK = false
		dump.Errors = append(dump.Errors, "upload readback does not match cached material upload")
	}
	for _, s := range atlasSamplePoints(upload.Width, upload.Height) {
		c := uploadImg.RGBAAt(s.x, s.y)
		dump.SampledSwatches = append(dump.SampledSwatches, atlasSwatchDump{
			Label: s.label,
			X:     s.x,
			Y:     s.y,
			R:     c.R,
			G:     c.G,
			B:     c.B,
			A:     c.A,
		})
	}
	switchSnapshots, switchReused := atlasRuntimeSwitchProbe(src)
	dump.RuntimeSwitch = switchSnapshots
	dump.RuntimeSwitchReused = &switchReused
	return sceneSpec{name: "atlas-" + string(preset), expected: expectedStats(1, 0, 1, 0, 1, 0)}, dump, nil
}

func buildAtlasTwoFSV(scene *core.Node, preset litasset.AtlasPreset) (sceneSpec, *atlasRuntimeDump, error) {
	srcA, err := syntheticAtlasSource("synthetic/faction-vigil.atlas.png", atlasPaletteVigil())
	if err != nil {
		return sceneSpec{}, nil, err
	}
	srcB, err := syntheticAtlasSource("synthetic/faction-ember.atlas.png", atlasPaletteEmber())
	if err != nil {
		return sceneSpec{}, nil, err
	}
	cache := litrender.NewAtlasMaterialCache()
	matA, err := cache.Material(srcA, preset)
	if err != nil {
		return sceneSpec{}, nil, err
	}
	matB, err := cache.Material(srcB, preset)
	if err != nil {
		return sceneSpec{}, nil, err
	}
	addAtlasPlane(scene, matA.Material, -370, 0, 620)
	addAtlasPlane(scene, matB.Material, 370, 0, 620)

	dump := &atlasRuntimeDump{
		Scene:  "atlas-two",
		Preset: preset,
		Source: atlasSourceDump{
			Name:   srcA.Name,
			Width:  srcA.Width,
			Height: srcA.Height,
			SHA256: srcA.SHA256,
		},
		Upload:              matA.Upload,
		Material:            cache.Snapshot(matA),
		AdditionalMaterials: []litrender.AtlasMaterialSnapshot{cache.Snapshot(matB)},
		MaterialInstances:   cache.Count(),
		OK:                  true,
	}
	if cache.Count() != 2 || matA == matB || matA.Texture == matB.Texture {
		dump.OK = false
		dump.Errors = append(dump.Errors, "two-atlas scene did not create exactly two distinct material/texture entries")
	}
	return sceneSpec{name: "atlas-two-" + string(preset), expected: expectedStats(2, 0, 2, 0, 2, 0)}, dump, nil
}

func addAtlasPlane(scene *core.Node, mat material.IMaterial, x, z, size float32) {
	plane := graphic.NewMesh(geometry.NewPlane(size, size), mat)
	plane.SetRotationX(-math32.Pi / 2)
	plane.SetPosition(x, 8, z)
	scene.Add(plane)
}

func syntheticAtlasSource(name string, palette []color.RGBA) (*litasset.AtlasSource, error) {
	img := image.NewRGBA(image.Rect(0, 0, litasset.AuthoredAtlasSize, litasset.AuthoredAtlasSize))
	swatches := []struct {
		rect image.Rectangle
		c    color.RGBA
	}{
		{image.Rect(0, 0, 512, 512), palette[0]},
		{image.Rect(512, 0, 1024, 512), palette[1]},
		{image.Rect(0, 512, 512, 1024), palette[2]},
		{image.Rect(512, 512, 1024, 1024), palette[3]},
	}
	for _, sw := range swatches {
		for y := sw.rect.Min.Y; y < sw.rect.Max.Y; y++ {
			for x := sw.rect.Min.X; x < sw.rect.Max.X; x++ {
				img.SetRGBA(x, y, sw.c)
			}
		}
	}
	return litasset.NewAtlasSource(name, img)
}

func atlasPaletteVigil() []color.RGBA {
	return []color.RGBA{
		{218, 56, 48, 255},
		{51, 151, 76, 255},
		{48, 99, 205, 255},
		{226, 194, 63, 255},
	}
}

func atlasPaletteEmber() []color.RGBA {
	return []color.RGBA{
		{136, 65, 210, 255},
		{44, 174, 190, 255},
		{231, 127, 42, 255},
		{236, 236, 232, 255},
	}
}

func atlasSamplePoints(w, h int) []struct {
	label string
	x, y  int
} {
	return []struct {
		label string
		x, y  int
	}{
		{label: "red-upper-left", x: w / 4, y: h / 4},
		{label: "green-upper-right", x: 3 * w / 4, y: h / 4},
		{label: "blue-lower-left", x: w / 4, y: 3 * h / 4},
		{label: "gold-lower-right", x: 3 * w / 4, y: 3 * h / 4},
	}
}

func atlasRuntimeSwitchProbe(src *litasset.AtlasSource) ([]litrender.AtlasMaterialSnapshot, bool) {
	cache := litrender.NewAtlasMaterialCache()
	high, _ := cache.Material(src, litasset.AtlasPresetHigh)
	medium, _ := cache.Material(src, litasset.AtlasPresetMedium)
	highAgain, _ := cache.Material(src, litasset.AtlasPresetHigh)
	out := []litrender.AtlasMaterialSnapshot{
		cache.Snapshot(high),
		cache.Snapshot(medium),
		cache.Snapshot(highAgain),
	}
	return out, high == highAgain && cache.Count() == 2
}

func buildTeamColorsFSV(scene *core.Node, preset litasset.AtlasPreset, name string) (sceneSpec, *teamColorRuntimeDump, error) {
	switch name {
	case "teamcolors", "teamcolors-one", "teamcolors-flash", "teamcolors-fade":
	default:
		return sceneSpec{}, nil, fmt.Errorf("unknown team-color scene %q", name)
	}

	src, err := syntheticTeamColorAtlasSource()
	if err != nil {
		return sceneSpec{}, nil, err
	}
	transparent := name == "teamcolors-fade"
	mat, upload, materialPath, materialInstances, err := buildTeamColorMaterial(src, preset, transparent)
	if err != nil {
		return sceneSpec{}, nil, err
	}

	dump := &teamColorRuntimeDump{
		Scene:             name,
		Preset:            preset,
		MaterialPath:      materialPath,
		Uniforms:          []string{"LitdTeamColor", "LitdTeamColorZone", "LitdFxScalars"},
		Upload:            upload,
		MaterialInstances: materialInstances,
		FlashIndex:        -1,
		FadeIndex:         -1,
		OK:                true,
	}

	geom := geometry.NewPlane(132, 92)
	const (
		columns = 7
		stepX   = float32(152)
		stepZ   = float32(132)
	)
	for i := 0; i < litrender.TeamColorSlots; i++ {
		slot := i
		if name == "teamcolors-one" {
			slot = 0
		}
		mesh, err := litrender.NewTeamColorMesh(geom, mat, slot)
		if err != nil {
			return sceneSpec{}, nil, err
		}
		mesh.SetRotationX(-math32.Pi / 2)
		x := float32(i%columns-3) * stepX
		z := float32(i/columns)*stepZ - stepZ/2
		mesh.SetPosition(x, 8, z)
		if name == "teamcolors-flash" && i == 4 {
			mesh.SetPresentationScalars(1, 1, 1)
			dump.FlashIndex = i
		}
		if name == "teamcolors-fade" && i == 5 {
			mesh.SetPresentationScalars(0, 0.5, 1)
			dump.FadeIndex = i
		}
		scene.Add(mesh)
		dump.Units = append(dump.Units, teamColorUnitDump{
			Index: i,
			X:     x,
			Z:     z,
			State: mesh.TeamColorState(),
		})
	}
	validateTeamColorDump(dump, name, preset)

	expected := expectedStats(litrender.TeamColorSlots, 0, litrender.TeamColorSlots, 0, 1, 0)
	if transparent {
		expected = expectedStats(litrender.TeamColorSlots, 0, 0, litrender.TeamColorSlots, 0, 1)
	}
	if preset == litasset.AtlasPresetLow {
		expected.Lights = 0
	}
	return sceneSpec{name: name + "-" + string(preset), expected: expected}, dump, nil
}

func buildTeamColorMaterial(src *litasset.AtlasSource, preset litasset.AtlasPreset, transparent bool) (material.IMaterial, litasset.AtlasUpload, string, int, error) {
	if preset == litasset.AtlasPresetLow {
		cache := litrender.NewAtlasMaterialCache()
		entry, err := cache.Material(src, preset)
		if err != nil {
			return nil, litasset.AtlasUpload{}, "", 0, err
		}
		entry.Material.SetUseLights(material.UseLightNone)
		entry.Material.SetSpecularColor(&math32.Color{R: 0, G: 0, B: 0})
		if transparent {
			entry.Material.SetTransparent(true)
		}
		return entry.Material, entry.Upload, "standard-unlit", cache.Count(), nil
	}

	uploadImg, upload, err := litasset.BuildAtlasUpload(src, preset)
	if err != nil {
		return nil, litasset.AtlasUpload{}, "", 0, err
	}
	tex := texture.NewTexture2DFromRGBA(uploadImg)
	tex.SetMagFilter(gls.LINEAR)
	tex.SetMinFilter(gls.LINEAR_MIPMAP_LINEAR)
	tex.SetWrapS(gls.CLAMP_TO_EDGE)
	tex.SetWrapT(gls.CLAMP_TO_EDGE)

	mat := material.NewPhysical()
	mat.SetBaseColorMap(tex)
	mat.SetMetallicFactor(0)
	mat.SetRoughnessFactor(1)
	if transparent {
		mat.SetTransparent(true)
	}
	return mat, upload, "physical", 1, nil
}

func syntheticTeamColorAtlasSource() (*litasset.AtlasSource, error) {
	img := image.NewRGBA(image.Rect(0, 0, litasset.AuthoredAtlasSize, litasset.AuthoredAtlasSize))
	fill := func(rect image.Rectangle, c color.RGBA) {
		for y := rect.Min.Y; y < rect.Max.Y; y++ {
			for x := rect.Min.X; x < rect.Max.X; x++ {
				img.SetRGBA(x, y, c)
			}
		}
	}

	fill(image.Rect(0, 0, 512, 1024), color.RGBA{R: 225, G: 225, B: 225, A: 255})
	fill(image.Rect(512, 0, 1024, 1024), color.RGBA{R: 78, G: 92, B: 108, A: 255})
	fill(image.Rect(512, 0, 1024, 256), color.RGBA{R: 218, G: 198, B: 150, A: 255})
	fill(image.Rect(512, 256, 1024, 512), color.RGBA{R: 58, G: 72, B: 86, A: 255})
	fill(image.Rect(512, 512, 1024, 768), color.RGBA{R: 150, G: 116, B: 73, A: 255})
	fill(image.Rect(512, 768, 1024, 1024), color.RGBA{R: 232, G: 229, B: 214, A: 255})
	return litasset.NewAtlasSource("synthetic/teamcolor-mask.atlas.png", img)
}

func validateTeamColorDump(dump *teamColorRuntimeDump, name string, preset litasset.AtlasPreset) {
	if dump.MaterialInstances != 1 {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("material instances = %d, want 1", dump.MaterialInstances))
	}
	if len(dump.Units) != litrender.TeamColorSlots {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("unit count = %d, want %d", len(dump.Units), litrender.TeamColorSlots))
	}
	wantSize, err := preset.Size()
	if err != nil {
		dump.OK = false
		dump.Errors = append(dump.Errors, err.Error())
		return
	}
	if dump.Upload.Width != wantSize || dump.Upload.Height != wantSize {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("upload size = %dx%d, want %dx%d", dump.Upload.Width, dump.Upload.Height, wantSize, wantSize))
	}
	for i, unit := range dump.Units {
		wantSlot := i
		if name == "teamcolors-one" {
			wantSlot = 0
		}
		if unit.State.Slot != wantSlot {
			dump.OK = false
			dump.Errors = append(dump.Errors, fmt.Sprintf("unit %d slot = %d, want %d", i, unit.State.Slot, wantSlot))
		}
		if unit.State.Zone != litrender.DefaultTeamColorZone() {
			dump.OK = false
			dump.Errors = append(dump.Errors, fmt.Sprintf("unit %d zone = %+v", i, unit.State.Zone))
		}
		if !unit.State.Enabled {
			dump.OK = false
			dump.Errors = append(dump.Errors, fmt.Sprintf("unit %d team color disabled", i))
		}
	}
	if name == "teamcolors-flash" && (dump.FlashIndex < 0 || dump.Units[dump.FlashIndex].State.HitFlash != 1) {
		dump.OK = false
		dump.Errors = append(dump.Errors, "flash scalar was not captured on the flash unit")
	}
	if name == "teamcolors-fade" && (dump.FadeIndex < 0 || dump.Units[dump.FadeIndex].State.FadeAlpha != 0.5) {
		dump.OK = false
		dump.Errors = append(dump.Errors, "fade scalar was not captured on the fade unit")
	}
}

// buildSpellstormFSV lights a dark ground plane with the fixed VFX light pool
// and scripts the 8-up-then-9th-eviction sequence, recording the acquire/evict
// event log and the final pool snapshot. lowPreset exercises the no-light path.
func buildSpellstormFSV(scene *core.Node, lowPreset bool) (sceneSpec, *vfxLightsRuntimeDump, error) {
	// Dark ground plane so the point lights read as distinct glows.
	plane := geometry.NewPlane(4000, 4000)
	mat := material.NewStandard(&math32.Color{R: 0.06, G: 0.06, B: 0.08})
	ground := graphic.NewMesh(plane, mat)
	ground.SetRotationX(-math32.Pi / 2) // lay flat on XZ
	scene.Add(ground)

	pool := litrender.NewVFXLightPool(scene, lowPreset)
	dump := &vfxLightsRuntimeDump{Scene: "spellstorm", LowPreset: lowPreset, OK: true}

	record := func(label string, r litrender.VFXRequest) {
		_, d := pool.Acquire(r)
		dump.Events = append(dump.Events, vfxLightEventDump{Request: label, Priority: r.Priority, Decision: d})
		if a := pool.ActiveCount(); a > dump.MaxActive {
			dump.MaxActive = a
		}
	}

	// 8 standard-spell lights spread across the plane.
	for i := 0; i < litrender.MaxVFXLights; i++ {
		ang := float32(i) / float32(litrender.MaxVFXLights) * 2 * math32.Pi
		x := 1200 * math32.Cos(ang)
		z := 1200 * math32.Sin(ang)
		record(fmt.Sprintf("standard#%d", i), litrender.VFXRequest{
			Priority: litrender.VFXStandardSpell, Lifetime: 200, Radius: 1400, ScreenDist: float32(i) * 100,
			Color: math32.Color{R: 0.4, G: 0.7, B: 1.0}, Intensity: 8, Pos: math32.Vector3{X: x, Y: 200, Z: z},
		})
	}
	// 9th request, higher priority → must evict the lowest-priority light.
	record("ultimate#9", litrender.VFXRequest{
		Priority: litrender.VFXUltimate, Lifetime: 200, Radius: 1600, ScreenDist: 0,
		Color: math32.Color{R: 1.0, G: 0.5, B: 0.2}, Intensity: 12, Pos: math32.Vector3{X: 0, Y: 300, Z: 0},
	})

	buf := make([]litrender.VFXSlotInfo, 0, litrender.MaxVFXLights)
	dump.Slots = pool.SnapshotInto(buf)
	dump.FinalActive = pool.ActiveCount()

	// Invariants for the autotest verdict.
	wantActive := litrender.MaxVFXLights
	if lowPreset {
		wantActive = 0
	}
	if dump.MaxActive > litrender.MaxVFXLights {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("pool exceeded cap: %d", dump.MaxActive))
	}
	if dump.FinalActive != wantActive {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("final active %d, want %d", dump.FinalActive, wantActive))
	}
	if !lowPreset {
		last := dump.Events[len(dump.Events)-1].Decision
		if !last.Granted || last.Victim < 0 {
			dump.OK = false
			dump.Errors = append(dump.Errors, "9th higher-priority request did not evict")
		}
	}
	// Hard build failures would return an error; invariant failures live in
	// dump.OK and surface through the autotest verdict.
	return sceneSpec{name: "spellstorm", expected: expectedStats(1, 0, 1, 0, 1, 0)}, dump, nil
}

func buildTerrainFSV(scene *core.Node, name string, wireframe bool) (sceneSpec, *terrainRuntimeDump, error) {
	name = strings.ToLower(strings.TrimSpace(name))
	if name == "" {
		name = "terrain"
	}
	if name == "terrain-chunks" {
		return buildTerrainChunksFSV(scene, wireframe)
	}
	if name != "terrain" && name != "terrain-units" {
		return sceneSpec{}, nil, fmt.Errorf("unknown terrain scene %q", name)
	}
	const mapPath = "data/maps/test64"
	m, err := litmapdata.Load(os.DirFS("."), mapPath)
	if err != nil {
		return sceneSpec{}, nil, err
	}
	mesh, err := litterrain.Build(m)
	if err != nil {
		return sceneSpec{}, nil, err
	}
	samples, maxDiff, err := litterrain.CompareHeights(mesh, m, litterrain.HundredVertexSamples(m.Width, m.Height))
	if err != nil {
		return sceneSpec{}, nil, err
	}
	mat := material.NewStandard(&math32.Color{R: 0.28, G: 0.45, B: 0.22})
	mat.SetSpecularColor(&math32.Color{R: 0.08, G: 0.08, B: 0.06})
	mat.SetWireframe(wireframe)
	terrainMesh := graphic.NewMesh(mesh.Geometry, mat)
	scene.Add(terrainMesh)

	dump := &terrainRuntimeDump{
		Scene:             name,
		MapPath:           mapPath,
		Wireframe:         wireframe,
		VertexCount:       mesh.VertexCount,
		TriangleCount:     mesh.TriangleCount,
		InvertedTriangles: mesh.InvertedTriangles(),
		MaxHeightDiff:     maxDiff,
		HeightSamples:     samples,
		OK:                true,
	}
	for _, p := range [][2]int{{0, 0}, {m.Width, 0}, {0, m.Height}, {m.Width, m.Height}} {
		pos, ok := mesh.PositionAtVertex(p[0], p[1])
		if !ok {
			dump.OK = false
			dump.Errors = append(dump.Errors, fmt.Sprintf("missing border vertex (%d,%d)", p[0], p[1]))
			continue
		}
		dump.BorderVertices = append(dump.BorderVertices, terrainBorderDump{
			X: p[0], Y: p[1], WorldX: pos.X, WorldY: pos.Y, WorldZ: pos.Z,
		})
	}
	if maxDiff != 0 {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("height max diff %d, want 0", maxDiff))
	}
	if dump.InvertedTriangles != 0 {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("%d inverted triangles", dump.InvertedTriangles))
	}
	if mesh.TriangleCount != m.Width*m.Height*2 {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("triangles %d, want %d", mesh.TriangleCount, m.Width*m.Height*2))
	}

	visible, opaqueStates := 1, 1
	if name == "terrain" && !wireframe {
		wireMat := material.NewStandard(&math32.Color{R: 0.06, G: 0.12, B: 0.05})
		wireMat.SetWireframe(true)
		scene.Add(graphic.NewMesh(mesh.Geometry, wireMat))
		visible++
		opaqueStates++
	}
	if name == "terrain-units" {
		addTerrainUnits(scene, mesh, dump)
		visible += len(dump.Units)
		opaqueStates++
	}
	return sceneSpec{name: name, expected: expectedStats(visible, 0, visible, 0, opaqueStates, 0)}, dump, nil
}

// buildTerrainChunksFSV bakes the map into 16×16-cell chunks and adds one Mesh
// (one draw call) per chunk, sharing a single terrain material. The dump
// reports the chunk grid, per-chunk triangle counts, and the seam-mismatch
// count across every shared chunk edge (must be 0 — no cracks). The rendered
// FrameStats in the enclosing scene dump carry the real draw-call/visible-graphic
// numbers for the ≤40-call terrain sub-budget check.
func buildTerrainChunksFSV(scene *core.Node, wireframe bool) (sceneSpec, *terrainRuntimeDump, error) {
	const mapPath = "data/maps/test64"
	m, err := litmapdata.Load(os.DirFS("."), mapPath)
	if err != nil {
		return sceneSpec{}, nil, err
	}
	cs, err := litterrain.BuildChunks(m, litterrain.ChunkCellSpan)
	if err != nil {
		return sceneSpec{}, nil, err
	}
	mat := material.NewStandard(&math32.Color{R: 0.28, G: 0.45, B: 0.22})
	mat.SetSpecularColor(&math32.Color{R: 0.08, G: 0.08, B: 0.06})
	mat.SetWireframe(wireframe)

	dump := &terrainRuntimeDump{
		Scene:      "terrain-chunks",
		MapPath:    mapPath,
		Wireframe:  wireframe,
		Chunked:    true,
		ChunkCells: cs.ChunkCells,
		ChunkCount: len(cs.Chunks),
		ChunkCols:  cs.Cols,
		ChunkRows:  cs.Rows,
		OK:         true,
	}
	totalVerts, totalTris := 0, 0
	for i := range cs.Chunks {
		c := &cs.Chunks[i]
		scene.Add(graphic.NewMesh(c.Geometry, mat))
		dump.ChunkTris = append(dump.ChunkTris, c.TriangleCount)
		if c.TriangleCount > dump.MaxChunkTris {
			dump.MaxChunkTris = c.TriangleCount
		}
		if c.TriangleCount > litterrain.MaxChunkTriangles {
			dump.OK = false
			dump.Errors = append(dump.Errors, fmt.Sprintf("chunk %d tris %d exceeds cap %d", i, c.TriangleCount, litterrain.MaxChunkTriangles))
		}
		if inv := c.InvertedTriangles(); inv != 0 {
			dump.OK = false
			dump.Errors = append(dump.Errors, fmt.Sprintf("chunk %d has %d inverted triangles", i, inv))
		}
		totalVerts += c.VertexCount
		totalTris += c.TriangleCount
	}
	dump.VertexCount = totalVerts
	dump.TriangleCount = totalTris
	if totalTris != m.Width*m.Height*2 {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("chunk tris sum %d, want whole-map %d", totalTris, m.Width*m.Height*2))
	}

	// Seam check: every vertex on a shared edge must resolve to the identical
	// world position in both adjacent chunks.
	dump.SeamMismatches = countSeamMismatches(cs)
	if dump.SeamMismatches != 0 {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("%d seam vertex mismatches (cracks)", dump.SeamMismatches))
	}

	// Border-corner sample vertices, same as the single-mesh scene, for the dump.
	for _, p := range [][2]int{{0, 0}, {m.Width, 0}, {0, m.Height}, {m.Width, m.Height}} {
		idx := cs.IndexOfVertexOwner(p[0], p[1])
		if idx < 0 {
			continue
		}
		if pos, ok := cs.Chunks[idx].WorldPosAt(p[0], p[1]); ok {
			dump.BorderVertices = append(dump.BorderVertices, terrainBorderDump{
				X: p[0], Y: p[1], WorldX: pos.X, WorldY: pos.Y, WorldZ: pos.Z,
			})
		}
	}

	visible := len(cs.Chunks)
	return sceneSpec{name: "terrain-chunks", expected: expectedStats(visible, 0, visible, 0, 1, 0)}, dump, nil
}

// countSeamMismatches compares the shared edge between each chunk and its right
// and bottom neighbour; any differing shared-edge vertex is a crack.
func countSeamMismatches(cs *litterrain.ChunkSet) int {
	mism := 0
	at := func(col, row int) *litterrain.Chunk {
		if col < 0 || row < 0 || col >= cs.Cols || row >= cs.Rows {
			return nil
		}
		return &cs.Chunks[row*cs.Cols+col]
	}
	for row := 0; row < cs.Rows; row++ {
		for col := 0; col < cs.Cols; col++ {
			c := at(col, row)
			if r := at(col+1, row); r != nil { // shared column gx = c.CellX1
				for gy := c.CellY0; gy <= c.CellY1; gy++ {
					cp, cok := c.WorldPosAt(c.CellX1, gy)
					rp, rok := r.WorldPosAt(c.CellX1, gy)
					if !cok || !rok || cp != rp {
						mism++
					}
				}
			}
			if b := at(col, row+1); b != nil { // shared row gy = c.CellY1
				for gx := c.CellX0; gx <= c.CellX1; gx++ {
					cp, cok := c.WorldPosAt(gx, c.CellY1)
					bp, bok := b.WorldPosAt(gx, c.CellY1)
					if !cok || !bok || cp != bp {
						mism++
					}
				}
			}
		}
	}
	return mism
}

func addTerrainUnits(scene *core.Node, mesh *litterrain.Mesh, dump *terrainRuntimeDump) {
	unitMat := material.NewStandard(&math32.Color{R: 0.24, G: 0.52, B: 0.92})
	unitGeom := geometry.NewBox(120, 160, 120)
	for _, u := range []struct {
		name string
		x, y int
	}{
		{name: "low-ground", x: 31, y: 32},
		{name: "slope-mid", x: 32, y: 32},
		{name: "high-ground", x: 33, y: 32},
		{name: "high-offset", x: 34, y: 34},
	} {
		pos, ok := mesh.PositionAtVertex(u.x, u.y)
		if !ok {
			dump.OK = false
			dump.Errors = append(dump.Errors, fmt.Sprintf("unit %s missing vertex (%d,%d)", u.name, u.x, u.y))
			continue
		}
		box := graphic.NewMesh(unitGeom, unitMat)
		box.SetPosition(pos.X, pos.Y+80, pos.Z)
		scene.Add(box)
		dump.Units = append(dump.Units, terrainUnitDump{
			Name: u.name, VertexX: u.x, VertexY: u.y, WorldX: pos.X, GroundY: pos.Y, WorldZ: pos.Z,
		})
	}
}

func addCameraRigScene(scene *core.Node) {
	groundMat := material.NewStandard(&math32.Color{R: 0.20, G: 0.44, B: 0.24})
	markerMat := material.NewStandard(&math32.Color{R: 0.82, G: 0.68, B: 0.30})
	ground := graphic.NewMesh(geometry.NewPlane(6400, 6400), groundMat)
	ground.SetRotationX(-math32.Pi / 2)
	scene.Add(ground)

	markerGeom := geometry.NewBox(90, 24, 90)
	addMesh(scene, markerGeom, markerMat, 0, 12, 0)
	addMesh(scene, markerGeom, markerMat, -320, 12, -320)
	addMesh(scene, markerGeom, markerMat, 320, 12, -320)
	addMesh(scene, markerGeom, markerMat, -320, 12, 320)
	addMesh(scene, markerGeom, markerMat, 320, 12, 320)
}

func addMesh(scene *core.Node, geom geometry.IGeometry, mat material.IMaterial, x, y, z float32) {
	mesh := graphic.NewMesh(geom, mat)
	mesh.SetPosition(x, y, z)
	scene.Add(mesh)
}

func addStatsHUD(scene *core.Node, spec sceneSpec) {
	text := fmt.Sprintf("scene=%s world=%d/%d draw=%d gui=%d state=%d",
		spec.name,
		spec.expected.VisibleGraphics,
		spec.expected.CulledGraphics,
		spec.expected.OpaqueDrawCalls+spec.expected.TransparentDrawCalls,
		spec.expected.GUIDrawCalls,
		spec.expected.StateChanges,
	)
	label := gui.NewLabel(text)
	label.SetPosition(14, 28)
	scene.Add(label)
}

type selectionFixtureEntity struct {
	Name  string
	World math32.Vector3
	Item  litinput.Selectable
}

func buildSelectionFSV(scene *core.Node, cameraRig *litrender.RTSCamera, res resolutionFlag, scenario string) (*selectionRuntimeDump, error) {
	scenario = strings.ToLower(strings.TrimSpace(scenario))
	if scenario == "" {
		scenario = "mixed"
	}
	switch scenario {
	case "mixed", "cap", "typesel":
	default:
		return nil, fmt.Errorf("unknown select-scenario %q", scenario)
	}

	names := []string{"mixed", "cap", "lowprio-mixed", "lowprio-workers", "shift-toggle", "enemy-click", "typesel", "tab"}
	dump := &selectionRuntimeDump{Scenario: scenario, OK: true}
	var currentItems []selectionFixtureEntity
	for _, name := range names {
		items := selectionFixtureItems(name)
		projectSelectionItems(cameraRig.Camera, res, items)
		c := runSelectionCase(name, items)
		dump.Cases = append(dump.Cases, c)
		if !c.OK {
			dump.OK = false
			dump.Errors = append(dump.Errors, fmt.Sprintf("%s selection mismatch", name))
		}
		if name == scenario {
			dump.Current = c
			currentItems = items
		}
	}
	drawSelectionFixture(scene, currentItems, dump.Current)
	return dump, nil
}

func selectionFixtureItems(name string) []selectionFixtureEntity {
	switch name {
	case "cap":
		out := make([]selectionFixtureEntity, 20)
		for i := range out {
			x := float32(-950 + i*100)
			out[i] = selectionFixtureEntity{
				Name:  fmt.Sprintf("unit-%02d", i+1),
				World: math32.Vector3{X: x, Y: 18, Z: 0},
				Item:  selectionItem(uint32(i+1), 1, litinput.SelectUnit, 0, false),
			}
		}
		return out
	case "typesel", "tab":
		return []selectionFixtureEntity{
			{Name: "footman-a", World: math32.Vector3{X: -220, Y: 18, Z: 0}, Item: selectionItem(1, 7, litinput.SelectUnit, 0, false)},
			{Name: "archer", World: math32.Vector3{X: -40, Y: 18, Z: 0}, Item: selectionItem(2, 8, litinput.SelectUnit, 0, false)},
			{Name: "footman-b", World: math32.Vector3{X: 140, Y: 18, Z: 0}, Item: selectionItem(3, 7, litinput.SelectUnit, 0, false)},
			{Name: "enemy-footman", World: math32.Vector3{X: 320, Y: 18, Z: 0}, Item: selectionItem(4, 7, litinput.SelectUnit, 1, false)},
		}
	case "lowprio-mixed", "lowprio-workers":
		return []selectionFixtureEntity{
			{Name: "worker-a", World: math32.Vector3{X: -160, Y: 18, Z: 0}, Item: selectionItem(1, 1, litinput.SelectUnit, 0, true)},
			{Name: "worker-b", World: math32.Vector3{X: 0, Y: 18, Z: 0}, Item: selectionItem(2, 1, litinput.SelectUnit, 0, true)},
			{Name: "footman", World: math32.Vector3{X: 160, Y: 18, Z: 0}, Item: selectionItem(3, 2, litinput.SelectUnit, 0, false)},
		}
	default:
		return []selectionFixtureEntity{
			{Name: "own-footman-a", World: math32.Vector3{X: -220, Y: 18, Z: 40}, Item: selectionItem(1, 10, litinput.SelectUnit, 0, false)},
			{Name: "own-footman-b", World: math32.Vector3{X: -40, Y: 18, Z: 40}, Item: selectionItem(2, 10, litinput.SelectUnit, 0, false)},
			{Name: "own-barracks", World: math32.Vector3{X: 160, Y: 35, Z: 40}, Item: selectionItem(3, 20, litinput.SelectBuilding, 0, false)},
			{Name: "enemy-grunt", World: math32.Vector3{X: -20, Y: 18, Z: -180}, Item: selectionItem(4, 10, litinput.SelectUnit, 1, false)},
			{Name: "enemy-tower", World: math32.Vector3{X: 240, Y: 35, Z: -180}, Item: selectionItem(5, 20, litinput.SelectBuilding, 1, false)},
		}
	}
}

func selectionItem(id uint32, typ uint16, class litinput.SelectClass, owner uint8, low bool) litinput.Selectable {
	return litinput.Selectable{ID: sim.EntityID(id), TypeID: typ, Class: class, OwnerPlayer: owner, LowPriority: low}
}

func projectSelectionItems(cam interface {
	Project(*math32.Vector3) *math32.Vector3
}, res resolutionFlag, items []selectionFixtureEntity) {
	for i := range items {
		p := items[i].World
		cam.Project(&p)
		x := (p.X + 1) * 0.5 * float32(res.W)
		y := (1 - p.Y) * 0.5 * float32(res.H)
		half := float32(18)
		if items[i].Item.Class == litinput.SelectBuilding {
			half = 30
		}
		items[i].Item.Screen = litinput.Rect{MinX: x - half, MinY: y - half, MaxX: x + half, MaxY: y + half}
	}
}

func runSelectionCase(name string, items []selectionFixtureEntity) selectionCaseDump {
	selectables := selectionSelectables(items)
	r := litinput.NewResolver(litinput.DefaultConfig(0))
	var res litinput.Result
	var gesture string
	var marquee litinput.Rect
	var clickX, clickY float32
	var expected []sim.EntityID

	switch name {
	case "shift-toggle":
		gesture = "shift-click toggle remove id=1, add id=3"
		r.SetSelection([]sim.EntityID{1, 2}, selectables)
		clickX, clickY = selectionCenter(items, 1)
		_ = r.Click(selectables, clickX, clickY, litinput.Modifiers{Shift: true})
		clickX, clickY = selectionCenter(items, 3)
		res = r.Click(selectables, clickX, clickY, litinput.Modifiers{Shift: true})
		expected = []sim.EntityID{2, 3}
	case "enemy-click":
		gesture = "enemy click view-only"
		clickX, clickY = selectionCenter(items, 4)
		res = r.Click(selectables, clickX, clickY, litinput.Modifiers{})
		expected = []sim.EntityID{4}
	case "typesel":
		gesture = "double-click type-select"
		clickX, clickY = selectionCenter(items, 1)
		res = r.Click(selectables, clickX, clickY, litinput.Modifiers{Double: true})
		expected = []sim.EntityID{1, 3}
	case "tab":
		gesture = "tab subgroup cycle"
		r.SetSelection([]sim.EntityID{1, 2, 3}, selectables)
		res = r.Tab(selectables)
		expected = []sim.EntityID{1, 2, 3}
	case "lowprio-mixed":
		gesture = "marquee workers plus normal unit"
		marquee = selectionBounds(items)
		res = r.Drag(selectables, marquee, litinput.Modifiers{})
		expected = []sim.EntityID{3}
	case "lowprio-workers":
		gesture = "marquee workers only"
		workerItems := items[:2]
		selectables = selectionSelectables(workerItems)
		marquee = selectionBounds(workerItems)
		res = r.Drag(selectables, marquee, litinput.Modifiers{})
		expected = []sim.EntityID{1, 2}
	case "cap":
		gesture = "20-unit marquee cap"
		marquee = selectionBounds(items)
		res = r.Drag(selectables, marquee, litinput.Modifiers{})
		expected = []sim.EntityID{10, 11, 9, 12, 8, 13, 7, 14, 15, 6, 5, 16}
	default:
		gesture = "mixed own units/buildings/enemies marquee"
		marquee = selectionBounds(items)
		res = r.Drag(selectables, marquee, litinput.Modifiers{})
		expected = []sim.EntityID{2, 1}
	}

	out := selectionCaseDump{
		Name:                  name,
		Gesture:               gesture,
		Marquee:               marquee,
		ClickX:                clickX,
		ClickY:                clickY,
		Selection:             selectionCommandIDs(selectionIDs(res.Selection)),
		Expected:              selectionCommandIDs(expected),
		ActiveSubgroup:        res.ActiveSubgroup,
		ActiveSubgroupTypeID:  res.ActiveSubgroupTypeID,
		Candidates:            res.Candidates,
		NormalPriority:        res.NormalPriority,
		CommandRecordsEmitted: res.CommandRecordsEmitted,
	}
	out.OK = sameEntityIDs(selectionIDs(res.Selection), expected)
	if name == "tab" {
		out.OK = out.OK && res.ActiveSubgroup == 1 && res.ActiveSubgroupTypeID == 8
	}
	return out
}

func selectionSelectables(items []selectionFixtureEntity) []litinput.Selectable {
	out := make([]litinput.Selectable, len(items))
	for i := range items {
		out[i] = items[i].Item
	}
	return out
}

func selectionBounds(items []selectionFixtureEntity) litinput.Rect {
	if len(items) == 0 {
		return litinput.Rect{}
	}
	r := items[0].Item.Screen
	for i := 1; i < len(items); i++ {
		s := items[i].Item.Screen
		if s.MinX < r.MinX {
			r.MinX = s.MinX
		}
		if s.MinY < r.MinY {
			r.MinY = s.MinY
		}
		if s.MaxX > r.MaxX {
			r.MaxX = s.MaxX
		}
		if s.MaxY > r.MaxY {
			r.MaxY = s.MaxY
		}
	}
	return r
}

func selectionCenter(items []selectionFixtureEntity, id sim.EntityID) (float32, float32) {
	for i := range items {
		if items[i].Item.ID == id {
			return items[i].Item.Screen.Center()
		}
	}
	return 0, 0
}

func selectionIDs(s litinput.Selection) []sim.EntityID {
	return s.IDs[:s.Count]
}

func selectionCommandIDs(ids []sim.EntityID) []uint32 {
	out := make([]uint32, 0, len(ids))
	for _, id := range ids {
		out = append(out, uint32(id))
	}
	return out
}

func sameEntityIDs(got, want []sim.EntityID) bool {
	if len(got) != len(want) {
		return false
	}
	for i := range want {
		if got[i] != want[i] {
			return false
		}
	}
	return true
}

func drawSelectionFixture(scene *core.Node, items []selectionFixtureEntity, current selectionCaseDump) {
	groundMat := material.NewStandard(&math32.Color{R: 0.18, G: 0.37, B: 0.23})
	ground := graphic.NewMesh(geometry.NewPlane(6400, 6400), groundMat)
	ground.SetRotationX(-math32.Pi / 2)
	scene.Add(ground)

	ownMat := material.NewStandard(&math32.Color{R: 0.18, G: 0.42, B: 0.92})
	workerMat := material.NewStandard(&math32.Color{R: 0.22, G: 0.62, B: 0.58})
	buildingMat := material.NewStandard(&math32.Color{R: 0.45, G: 0.52, B: 0.62})
	enemyMat := material.NewStandard(&math32.Color{R: 0.88, G: 0.24, B: 0.20})
	selectedMat := material.NewStandard(&math32.Color{R: 1.0, G: 0.82, B: 0.18})

	for _, ent := range items {
		mat := ownMat
		if ent.Item.OwnerPlayer != 0 {
			mat = enemyMat
		} else if ent.Item.Class == litinput.SelectBuilding {
			mat = buildingMat
		} else if ent.Item.LowPriority {
			mat = workerMat
		}
		size := float32(62)
		height := float32(36)
		if ent.Item.Class == litinput.SelectBuilding {
			size = 118
			height = 72
		}
		mesh := graphic.NewMesh(geometry.NewBox(size, height, size), mat)
		mesh.SetPosition(ent.World.X, height*0.5, ent.World.Z)
		scene.Add(mesh)
		if selectionCaseContains(current.Selection, uint32(ent.Item.ID)) {
			ring := graphic.NewMesh(geometry.NewTorus(float64(size*0.64), 4, 28, 8, math32.Pi*2), selectedMat)
			ring.SetRotationX(math32.Pi / 2)
			ring.SetPosition(ent.World.X, 3, ent.World.Z)
			scene.Add(ring)
		}
	}

	if current.Marquee.MaxX != 0 || current.Marquee.MaxY != 0 {
		drawSelectionMarquee(scene, current.Marquee)
	}
}

func drawSelectionMarquee(scene *core.Node, rect litinput.Rect) {
	rect = litinput.NormalizeRect(rect)
	color := math32.Color4{R: 0.92, G: 0.84, B: 0.22, A: 0.90}
	thick := float32(3)
	add := func(x, y, w, h float32) {
		p := gui.NewPanel(w, h)
		p.SetColor4(&color)
		p.SetPosition(x, y)
		scene.Add(p)
	}
	w := rect.MaxX - rect.MinX
	h := rect.MaxY - rect.MinY
	add(rect.MinX, rect.MinY, w, thick)
	add(rect.MinX, rect.MaxY-thick, w, thick)
	add(rect.MinX, rect.MinY, thick, h)
	add(rect.MaxX-thick, rect.MinY, thick, h)
}

func selectionCaseContains(ids []uint32, id uint32) bool {
	for _, got := range ids {
		if got == id {
			return true
		}
	}
	return false
}

func buildGroupFSV(scene *core.Node, cameraRig *litrender.RTSCamera) *groupRuntimeDump {
	items := groupFixtureItems()
	liveAfterDeaths := groupFixtureLiveAfterDeaths(items)
	live := groupLiveEntities(liveAfterDeaths)
	groups := litinput.NewControlGroups(litinput.DefaultGroupConfig())
	dump := &groupRuntimeDump{OK: true}
	addCase := func(c groupCaseDump) {
		dump.Cases = append(dump.Cases, c)
		if !c.OK {
			dump.OK = false
			dump.Errors = append(dump.Errors, fmt.Sprintf("%s group mismatch", c.Name))
		}
	}

	assign := groups.Assign(1, groupSelection(1, 2, 3, 4, 5))
	addCase(groupCase("assign-5", "Ctrl+1 assign five units", groups, assign, []sim.EntityID{1, 2, 3, 4, 5}, []sim.EntityID{1, 2, 3, 4, 5}, 0, false, 0, 0))

	recall := groups.Recall(1, live, 1000)
	addCase(groupCase("recall-pruned", "1 recall after ids 2 and 4 died", groups, recall, []sim.EntityID{1, 3, 5}, []sim.EntityID{1, 3, 5}, 2, false, 0, 0))

	center := groups.Recall(1, live, 1299)
	addCase(groupCase("doubletap-299", "1 then 1 at 299 ms recalls and centers", groups, center, []sim.EntityID{1, 3, 5}, []sim.EntityID{1, 3, 5}, 0, true, 120, 80))
	dump.Current = dump.Cases[len(dump.Cases)-1]
	cameraRig.SetAnchor(math32.Vector3{X: center.CenterX, Z: center.CenterZ})

	_ = groups.Recall(1, live, 2000)
	late := groups.Recall(1, live, 2350)
	addCase(groupCase("doubletap-350", "1 then 1 at 350 ms recalls without camera center", groups, late, []sim.EntityID{1, 3, 5}, []sim.EntityID{1, 3, 5}, 0, false, 0, 0))

	added := groups.Add(1, groupSelection(6, 7, 8))
	addCase(groupCase("shift-add", "Shift+1 adds three more units without changing selection", groups, added, []sim.EntityID{6, 7, 8}, []sim.EntityID{1, 3, 5, 6, 7, 8}, 0, false, 0, 0))

	reassigned := groups.Assign(1, groupSelection(8))
	addCase(groupCase("ctrl-reassign", "Ctrl+1 replaces old contents", groups, reassigned, []sim.EntityID{8}, []sim.EntityID{8}, 0, false, 0, 0))

	old := sim.EntityID(7)
	recycled := sim.EntityID(1<<24 | 7)
	genGroups := litinput.NewControlGroups(litinput.DefaultGroupConfig())
	genGroups.Assign(2, groupSelectionIDs(old))
	gen := genGroups.Recall(2, []litinput.GroupEntity{{ID: recycled, X: 520, Z: 80}}, 1000)
	genCase := groupCase("generation-reuse", "recycled slot keeps old group stale", genGroups, gen, nil, nil, 1, false, 0, 0)
	genCase.OldID = uint32(old)
	genCase.RecycledID = uint32(recycled)
	addCase(genCase)

	drawSelectionFixture(scene, liveAfterDeaths, selectionCaseDump{Name: "group-recall", Selection: dump.Current.Selection})
	return dump
}

func buildSmartOrderFSV(scene *core.Node) (*orderRuntimeDump, error) {
	w, workerA, fighter, workerB, mine, err := renderSmartOrderWorld()
	if err != nil {
		return nil, err
	}
	dump := &orderRuntimeDump{Scenario: "economy", OK: true}
	addCase := func(c orderCaseDump) {
		dump.Cases = append(dump.Cases, c)
		if !c.OK {
			dump.OK = false
			dump.Errors = append(dump.Errors, fmt.Sprintf("%s smart-order mismatch", c.Name))
		}
	}

	selection := renderOrderSelection(workerA, fighter, workerB)
	resourceReq := litinput.SmartOrderRequest{
		Player:    0,
		Team:      0,
		Seq:       21,
		Selection: selection,
		Target: litinput.SmartTarget{
			Entity:   mine,
			Point:    fixed.Vec2{X: fixed.FromInt(1024), Y: fixed.FromInt(768)},
			Class:    litdata.TCResource,
			ClassSet: true,
		},
	}
	harvest := renderSmartOrderCase(w, "harvest-split", "right-click gold mine with workers + escort",
		"resource", resourceReq, []uint8{sim.OpHarvest, sim.OpMove}, litinput.SmartFeedbackNone)
	addCase(harvest)

	hidden := resourceReq
	hidden.Selection = renderOrderSelection(fighter)
	hidden.Target = litinput.SmartTarget{
		Entity:   mine,
		Point:    fixed.Vec2{X: fixed.FromInt(1200), Y: fixed.FromInt(700)},
		Class:    litdata.TCEnemy,
		ClassSet: true,
		Hidden:   true,
	}
	addCase(renderSmartOrderCase(w, "hidden-enemy", "right-click fog-hidden enemy",
		"enemy", hidden, nil, litinput.SmartFeedbackHiddenTarget))

	dead, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(1300), Y: fixed.FromInt(700)}, 0)
	if !ok {
		return nil, fmt.Errorf("dead target setup failed")
	}
	w.DestroyUnit(dead)
	deadReq := resourceReq
	deadReq.Selection = renderOrderSelection(fighter)
	deadReq.Target = litinput.SmartTarget{
		Entity:   dead,
		Point:    fixed.Vec2{X: fixed.FromInt(1300), Y: fixed.FromInt(700)},
		Class:    litdata.TCEnemy,
		ClassSet: true,
	}
	addCase(renderSmartOrderCase(w, "dead-target", "right-click target destroyed before encode",
		"enemy", deadReq, nil, litinput.SmartFeedbackDeadTarget))

	dump.Current = harvest
	drawSmartOrderFixture(scene)
	return dump, nil
}

func renderSmartOrderWorld() (*sim.World, sim.EntityID, sim.EntityID, sim.EntityID, sim.EntityID, error) {
	tables, err := litdata.Load(os.DirFS("data"))
	if err != nil {
		return nil, 0, 0, 0, 0, err
	}
	w := sim.NewWorld(sim.Caps{})
	if !w.BindSmartTable(tables.Smart, []uint8{0, 1}) {
		return nil, 0, 0, 0, 0, fmt.Errorf("BindSmartTable failed")
	}
	workerA, ok := renderSmartOrderUnit(w, 1)
	if !ok {
		return nil, 0, 0, 0, 0, fmt.Errorf("workerA setup failed")
	}
	fighter, ok := renderSmartOrderUnit(w, 0)
	if !ok {
		return nil, 0, 0, 0, 0, fmt.Errorf("fighter setup failed")
	}
	workerB, ok := renderSmartOrderUnit(w, 1)
	if !ok {
		return nil, 0, 0, 0, 0, fmt.Errorf("workerB setup failed")
	}
	mine, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(1024), Y: fixed.FromInt(768)}, 0)
	if !ok {
		return nil, 0, 0, 0, 0, fmt.Errorf("mine target setup failed")
	}
	return w, workerA, fighter, workerB, mine, nil
}

func renderSmartOrderUnit(w *sim.World, typeID uint16) (sim.EntityID, bool) {
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.One, Y: fixed.One}, 0)
	if !ok {
		return 0, false
	}
	return id, w.Owners.Add(w.Ents, id, 0, 0, 0) && w.UnitTypes.Add(w.Ents, id, typeID)
}

func renderOrderSelection(ids ...sim.EntityID) litinput.Selection {
	var s litinput.Selection
	for _, id := range ids {
		if s.Count >= sim.MaxCommandUnits {
			break
		}
		s.IDs[s.Count] = id
		s.Count++
	}
	return s
}

func renderSmartOrderCase(w *sim.World, name, gesture, targetClass string, req litinput.SmartOrderRequest, wantOps []uint8, wantFeedback litinput.SmartFeedback) orderCaseDump {
	var out litinput.SmartOrderResult
	encoded, ok := litinput.ResolveRightClick(w, req, make([]byte, 0, 256), &out)
	c := orderCaseDump{
		Name:            name,
		Gesture:         gesture,
		TargetClass:     targetClass,
		Selection:       selectionCommandIDs(selectionIDs(req.Selection)),
		Target:          uint32(req.Target.Entity),
		Feedback:        out.Feedback.String(),
		EncodedBytes:    len(encoded),
		Records:         make([]commandRecordDump, 0, out.Count),
		ExpectedOpcodes: opcodesForJSON(wantOps),
		OK:              out.Feedback == wantFeedback && int(out.Count) == len(wantOps),
	}
	if wantFeedback == litinput.SmartFeedbackNone {
		c.OK = c.OK && ok
	} else {
		c.OK = c.OK && !ok
	}
	for i := uint8(0); i < out.Count; i++ {
		rec := commandRecordDumpFor(out.Records[i])
		c.Records = append(c.Records, rec)
		if int(i) >= len(wantOps) || rec.Opcode != wantOps[i] {
			c.OK = false
		}
	}
	return c
}

func opcodesForJSON(ops []uint8) []uint32 {
	if ops == nil {
		return []uint32{}
	}
	out := make([]uint32, 0, len(ops))
	for _, op := range ops {
		out = append(out, uint32(op))
	}
	return out
}

type queueHappyRun struct {
	unit           sim.EntityID
	records        []sim.CommandRecord
	trace          []queueTraceDump
	screenshot     queueTraceDump
	collapseBefore queueTraceDump
	collapseAfter  queueTraceDump
	final          queueTraceDump
	collapseTick   uint32
}

func buildQueueFSV(scene *core.Node) (*queueRuntimeDump, error) {
	first, err := runQueueHappyPath()
	if err != nil {
		return nil, err
	}
	second, err := runQueueHappyPath()
	if err != nil {
		return nil, err
	}
	overflow := buildQueueOverflowCase()
	dead := buildQueueDeadCleanupCase()
	collapse := queueCaseDump{
		Name:     "unmodified-collapse",
		Before:   first.collapseBefore,
		After:    first.collapseAfter,
		Expected: "plain unmodified order clears queued FIFO and leaves exactly one active current order",
		OK: first.collapseBefore.QueueDepth > 0 &&
			first.collapseAfter.QueueDepth == 0 &&
			first.collapseAfter.TotalOrders == 1 &&
			first.collapseAfter.Current.Kind == sim.OrderMove &&
			first.collapseAfter.Current.Point == queuePoint(queueSecondSequence()[0]),
	}
	if !collapse.OK {
		collapse.Error = fmt.Sprintf("before depth=%d after depth=%d total=%d kind=%d point=%+v",
			first.collapseBefore.QueueDepth, first.collapseAfter.QueueDepth,
			first.collapseAfter.TotalOrders, first.collapseAfter.Current.Kind,
			first.collapseAfter.Current.Point)
	}

	dump := &queueRuntimeDump{
		Scenario:        "moveorder",
		Unit:            uint32(first.unit),
		InitialSequence: queuePointList(queueInitialSequence()),
		SecondSequence:  queuePointList(queueSecondSequence()),
		Trace:           first.trace,
		ScreenshotState: first.screenshot,
		FinalState:      first.final,
		Replay: queueReplayDump{
			FirstHash:        first.final.Hash,
			SecondHash:       second.final.Hash,
			Equal:            first.final.Hash == second.final.Hash && first.final.Pos == second.final.Pos,
			FirstFinalPos:    first.final.Pos,
			SecondFinalPos:   second.final.Pos,
			CollapseAtTick:   first.collapseTick,
			CommandsReplayed: len(first.records),
		},
		Cases: []queueCaseDump{collapse, overflow, dead},
		OK:    true,
	}
	for _, rec := range first.records {
		dump.Records = append(dump.Records, commandRecordDumpFor(rec))
	}
	if len(first.records) > 1 {
		encoded, ok := sim.AppendEncode(nil, &first.records[1])
		if ok {
			dump.QueuedFlagHex = fmt.Sprintf("%x", encoded)
			if len(encoded) > 9 {
				dump.QueuedFlagByte = encoded[9]
			}
		}
	}
	if dump.QueuedFlagByte != sim.CmdFlagQueued {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("queued flag byte=%02x want %02x", dump.QueuedFlagByte, sim.CmdFlagQueued))
	}
	if !dump.Replay.Equal {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("replay hash/position diverged: %s/%+v vs %s/%+v",
			dump.Replay.FirstHash, dump.Replay.FirstFinalPos, dump.Replay.SecondHash, dump.Replay.SecondFinalPos))
	}
	if first.final.Pos != queuePoint(queueSecondSequence()[0]) {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("final position=%+v want %+v", first.final.Pos, queuePoint(queueSecondSequence()[0])))
	}
	for _, c := range dump.Cases {
		if !c.OK {
			dump.OK = false
			if c.Error != "" {
				dump.Errors = append(dump.Errors, c.Name+": "+c.Error)
			} else {
				dump.Errors = append(dump.Errors, c.Name+" failed")
			}
		}
	}
	drawQueueFixture(scene, first.screenshot, queueInitialSequence(), queueSecondSequence())
	return dump, nil
}

func runQueueHappyPath() (queueHappyRun, error) {
	w := sim.NewWorld(sim.Caps{})
	unit, err := queueFixtureUnit(w, fixed.Vec2{}, 16*fixed.One)
	if err != nil {
		return queueHappyRun{}, err
	}
	reg, snap := sim.NewHashRegistry(), &statehash.Snapshot{}
	run := queueHappyRun{unit: unit}
	capture := func(label string) queueTraceDump {
		t := queueTrace(w, unit, label, reg, snap)
		run.trace = append(run.trace, t)
		return t
	}
	capture("before")

	initial := queueInitialSequence()
	for i, pt := range initial {
		queued := i > 0
		rec := queueMoveRecord(unit, uint16(i), queued, pt)
		run.records = append(run.records, rec)
		w.StageCommand(rec)
	}
	w.IngestStagedCommands()
	afterIngest := queueStepCapture(w, unit, "after-queued-ingest", reg, snap)
	run.trace = append(run.trace, afterIngest)
	if afterIngest.QueueDepth != 4 {
		return queueHappyRun{}, fmt.Errorf("queued ingest depth=%d want 4: %+v", afterIngest.QueueDepth, afterIngest)
	}

	var shot queueTraceDump
	for i := 0; i < 160; i++ {
		label := fmt.Sprintf("queue-drain-%02d", i+1)
		t := queueStepCapture(w, unit, label, reg, snap)
		run.trace = append(run.trace, t)
		if shot.Label == "" &&
			t.QueueDepth <= 2 &&
			t.MoveState == sim.MoveFollowing &&
			t.Current.Kind == sim.OrderMove &&
			t.Pos != t.Current.Point {
			shot = t
			shot.Label = "screenshot-mid-route"
			run.trace[len(run.trace)-1] = shot
			break
		}
	}
	if shot.Label == "" {
		return queueHappyRun{}, fmt.Errorf("never reached a mid-route state after queued waypoint drain")
	}
	run.screenshot = shot
	run.collapseBefore = queueTrace(w, unit, "before-plain-click", reg, snap)
	run.trace = append(run.trace, run.collapseBefore)

	second := queueSecondSequence()[0]
	rec := queueMoveRecord(unit, uint16(len(run.records)), false, second)
	run.records = append(run.records, rec)
	w.StageCommand(rec)
	w.IngestStagedCommands()
	run.collapseAfter = queueStepCapture(w, unit, "after-plain-click-collapse", reg, snap)
	run.collapseTick = run.collapseAfter.Tick
	run.trace = append(run.trace, run.collapseAfter)
	if run.collapseAfter.QueueDepth != 0 || run.collapseAfter.TotalOrders != 1 {
		return queueHappyRun{}, fmt.Errorf("plain click did not collapse queue: %+v", run.collapseAfter)
	}

	for i := 0; i < 180; i++ {
		t := queueStepCapture(w, unit, fmt.Sprintf("second-sequence-%02d", i+1), reg, snap)
		run.trace = append(run.trace, t)
		if t.Pos == queuePoint(second) && t.QueueDepth == 0 && t.Current.Kind == sim.OrderStop {
			run.final = t
			run.final.Label = "final"
			run.trace[len(run.trace)-1] = run.final
			return run, nil
		}
	}
	return queueHappyRun{}, fmt.Errorf("unit did not finish second sequence at %+v", queuePoint(second))
}

func buildQueueOverflowCase() queueCaseDump {
	w := sim.NewWorld(sim.Caps{})
	unit, err := queueFixtureUnit(w, fixed.Vec2{}, 8*fixed.One)
	if err != nil {
		return queueCaseDump{Name: "overflow-20-shift-orders", OK: false, Error: err.Error()}
	}
	var drops []queueEventDump
	w.RegisterHandler(9001, func(ww *sim.World, e sim.Event) {
		if e.Src == unit && e.Kind == sim.EvOrderDropped {
			drops = append(drops, queueEventDump{Tick: ww.Tick(), Kind: e.Kind, Src: uint32(e.Src), Arg: e.Arg})
		}
	})
	w.Subscribe(sim.EvOrderDropped, 9001)
	reg, snap := sim.NewHashRegistry(), &statehash.Snapshot{}
	before := queueTrace(w, unit, "before-overflow", reg, snap)
	first := queueMoveRecord(unit, 0, false, fixed.Vec2{X: fixed.FromInt(64), Y: 0})
	w.StageCommand(first)
	for i := 0; i < 20; i++ {
		pt := fixed.Vec2{X: fixed.FromInt(64), Y: fixed.FromInt(int32(16 * (i + 1)))}
		w.StageCommand(queueMoveRecord(unit, uint16(i+1), true, pt))
	}
	w.IngestStagedCommands()
	after := queueStepCapture(w, unit, "after-overflow", reg, snap)
	c := queueCaseDump{
		Name:     "overflow-20-shift-orders",
		Before:   before,
		After:    after,
		Drops:    drops,
		Expected: "20 shifted appends keep 16 FIFO entries and emit 4 deterministic drops",
		OK:       after.QueueDepth == sim.MaxOrderQueue && len(drops) == 4,
	}
	if !c.OK {
		c.Error = fmt.Sprintf("depth=%d drops=%d want depth=%d drops=4", after.QueueDepth, len(drops), sim.MaxOrderQueue)
	}
	return c
}

func buildQueueDeadCleanupCase() queueCaseDump {
	w := sim.NewWorld(sim.Caps{})
	unit, err := queueFixtureUnit(w, fixed.Vec2{}, 8*fixed.One)
	if err != nil {
		return queueCaseDump{Name: "dead-unit-cleanup", OK: false, Error: err.Error()}
	}
	reg, snap := sim.NewHashRegistry(), &statehash.Snapshot{}
	baseFree := w.OrderPoolFree()
	first := queueMoveRecord(unit, 0, false, fixed.Vec2{X: fixed.FromInt(96), Y: 0})
	w.StageCommand(first)
	for i := 0; i < 3; i++ {
		pt := fixed.Vec2{X: fixed.FromInt(96), Y: fixed.FromInt(int32(32 * (i + 1)))}
		w.StageCommand(queueMoveRecord(unit, uint16(i+1), true, pt))
	}
	w.IngestStagedCommands()
	before := queueStepCapture(w, unit, "before-destroy", reg, snap)
	w.DestroyUnit(unit)
	after := queueTrace(w, unit, "after-destroy", reg, snap)
	c := queueCaseDump{
		Name:         "dead-unit-cleanup",
		Before:       before,
		After:        after,
		Expected:     "destroying a unit with queued orders discards entries and restores the pool",
		PoolFreeBase: baseFree,
		OK:           before.QueueDepth == 3 && after.QueueDepth == 0 && after.TotalOrders == 0 && !after.Alive && w.OrderPoolFree() == baseFree,
	}
	if !c.OK {
		c.Error = fmt.Sprintf("before depth=%d after depth=%d alive=%v total=%d pool=%d wantPool=%d",
			before.QueueDepth, after.QueueDepth, after.Alive, after.TotalOrders, w.OrderPoolFree(), baseFree)
	}
	return c
}

func queueFixtureUnit(w *sim.World, pos fixed.Vec2, speed fixed.F64) (sim.EntityID, error) {
	id, ok := w.CreateUnit(pos, 0)
	if !ok {
		return 0, fmt.Errorf("unit create failed")
	}
	if !w.Owners.Add(w.Ents, id, 0, 0, 0) ||
		!w.Movements.Add(w.Ents, w.Transforms, id, speed, 0x4000) ||
		!w.Orders.Add(w.Ents, id) {
		return 0, fmt.Errorf("unit component setup failed")
	}
	return id, nil
}

func queueInitialSequence() []fixed.Vec2 {
	return []fixed.Vec2{
		{X: fixed.FromInt(96), Y: 0},
		{X: fixed.FromInt(96), Y: fixed.FromInt(64)},
		{X: fixed.FromInt(32), Y: fixed.FromInt(64)},
		{X: fixed.FromInt(32), Y: fixed.FromInt(128)},
		{X: fixed.FromInt(160), Y: fixed.FromInt(128)},
	}
}

func queueSecondSequence() []fixed.Vec2 {
	return []fixed.Vec2{{X: fixed.FromInt(220), Y: fixed.FromInt(32)}}
}

func queueMoveRecord(unit sim.EntityID, seq uint16, queued bool, pt fixed.Vec2) sim.CommandRecord {
	flags := uint8(0)
	if queued {
		flags = sim.CmdFlagQueued
	}
	rec := sim.CommandRecord{
		Version:   sim.CommandVersion,
		Player:    0,
		Seq:       seq,
		Opcode:    sim.OpMove,
		Flags:     flags,
		UnitCount: 1,
		Point:     pt,
	}
	rec.Units[0] = unit
	return rec
}

func queueStepCapture(w *sim.World, unit sim.EntityID, label string, reg *statehash.Registry, snap *statehash.Snapshot) queueTraceDump {
	w.Step()
	return queueTrace(w, unit, label, reg, snap)
}

func queueTrace(w *sim.World, unit sim.EntityID, label string, reg *statehash.Registry, snap *statehash.Snapshot) queueTraceDump {
	pos := fixed.Vec2{}
	if tr := w.Transforms.Row(unit); tr != -1 {
		pos = w.Transforms.Pos[tr]
	}
	moveState := uint8(255)
	if mr := w.Movements.Row(unit); mr != -1 {
		moveState = w.Movements.State[mr]
	}
	current, hasOrder := w.CurrentOrder(unit)
	queue := w.AppendOrderQueue(unit, nil)
	out := queueTraceDump{
		Label:          label,
		Tick:           w.Tick(),
		Alive:          w.Ents.Alive(unit),
		HasOrder:       hasOrder,
		Pos:            queuePoint(pos),
		MoveState:      moveState,
		QueueDepth:     len(queue),
		OrderPoolFree:  w.OrderPoolFree(),
		PathQueueDepth: w.PathQueueDepth(),
		PathExpansions: w.PathExpansionsLastTick(),
		Hash:           queueHash(w, reg, snap),
	}
	if hasOrder {
		out.Current = queueOrder(current)
		out.TotalOrders = 1 + len(queue)
	}
	out.Queue = make([]queueOrderDump, 0, len(queue))
	for _, order := range queue {
		out.Queue = append(out.Queue, queueOrder(order))
	}
	return out
}

func queueHash(w *sim.World, reg *statehash.Registry, snap *statehash.Snapshot) string {
	w.HashState(reg, snap)
	return fmt.Sprintf("%016x", snap.Top)
}

func queueOrder(o sim.Order) queueOrderDump {
	return queueOrderDump{
		Kind:   o.Kind,
		Target: uint32(o.Target),
		Point:  queuePoint(o.Point),
		Data:   o.Data,
	}
}

func queuePoint(p fixed.Vec2) queuePointDump {
	return queuePointDump{
		XRaw: int64(p.X),
		YRaw: int64(p.Y),
		X:    p.X.Floor(),
		Y:    p.Y.Floor(),
	}
}

func queuePointList(points []fixed.Vec2) []queuePointDump {
	out := make([]queuePointDump, 0, len(points))
	for _, p := range points {
		out = append(out, queuePoint(p))
	}
	return out
}

func drawQueueFixture(scene *core.Node, shot queueTraceDump, initial, second []fixed.Vec2) {
	groundMat := material.NewStandard(&math32.Color{R: 0.18, G: 0.27, B: 0.22})
	ground := graphic.NewMesh(geometry.NewPlane(520, 360), groundMat)
	ground.SetRotationX(-math32.Pi / 2)
	scene.Add(ground)

	startMat := material.NewStandard(&math32.Color{R: 0.20, G: 0.55, B: 0.90})
	queuedMat := material.NewStandard(&math32.Color{R: 0.92, G: 0.70, B: 0.20})
	finalMat := material.NewStandard(&math32.Color{R: 0.25, G: 0.78, B: 0.42})
	unitMat := material.NewStandard(&math32.Color{R: 0.88, G: 0.30, B: 0.24})
	currentMat := material.NewStandard(&math32.Color{R: 0.96, G: 0.95, B: 0.68})

	addQueueMarker(scene, fixed.Vec2{}, startMat, 42, 18)
	for i, p := range initial {
		mat := queuedMat
		if i == 0 {
			mat = startMat
		}
		addQueueMarker(scene, p, mat, 34, 16)
	}
	for _, p := range second {
		addQueueMarker(scene, p, finalMat, 44, 18)
	}
	unitPos := fixed.Vec2{X: fixed.F64(shot.Pos.XRaw), Y: fixed.F64(shot.Pos.YRaw)}
	x, z := queueRenderXZ(unitPos)
	addOrderMesh(scene, geometry.NewBox(42, 58, 42), unitMat, x, 29, z)
	current := fixed.Vec2{X: fixed.F64(shot.Current.Point.XRaw), Y: fixed.F64(shot.Current.Point.YRaw)}
	cx, cz := queueRenderXZ(current)
	ring := graphic.NewMesh(geometry.NewTorus(30, 4, 28, 8, math32.Pi*2), currentMat)
	ring.SetRotationX(math32.Pi / 2)
	ring.SetPosition(cx, 5, cz)
	scene.Add(ring)
}

func addQueueMarker(scene *core.Node, pt fixed.Vec2, mat material.IMaterial, size, height float32) {
	x, z := queueRenderXZ(pt)
	addOrderMesh(scene, geometry.NewBox(size, height, size), mat, x, height/2, z)
}

func queueRenderXZ(pt fixed.Vec2) (float32, float32) {
	return float32(pt.X.Floor()) - 110, float32(pt.Y.Floor()) - 70
}

func drawSmartOrderFixture(scene *core.Node) {
	groundMat := material.NewStandard(&math32.Color{R: 0.20, G: 0.32, B: 0.24})
	ground := graphic.NewMesh(geometry.NewPlane(1800, 1200), groundMat)
	ground.SetRotationX(-math32.Pi / 2)
	scene.Add(ground)

	mineMat := material.NewStandard(&math32.Color{R: 0.96, G: 0.72, B: 0.18})
	workerMat := material.NewStandard(&math32.Color{R: 0.20, G: 0.58, B: 0.90})
	escortMat := material.NewStandard(&math32.Color{R: 0.28, G: 0.78, B: 0.38})
	ringMat := material.NewStandard(&math32.Color{R: 0.95, G: 0.86, B: 0.20})

	addOrderMesh(scene, geometry.NewBox(180, 120, 180), mineMat, 0, 60, 0)
	addOrderMesh(scene, geometry.NewBox(70, 80, 70), workerMat, -120, 40, 120)
	addOrderMesh(scene, geometry.NewBox(70, 80, 70), workerMat, 120, 40, 120)
	addOrderMesh(scene, geometry.NewBox(82, 96, 82), escortMat, 260, 48, -130)
	ring := graphic.NewMesh(geometry.NewTorus(180, 5, 32, 8, math32.Pi*2), ringMat)
	ring.SetRotationX(math32.Pi / 2)
	ring.SetPosition(0, 4, 0)
	scene.Add(ring)
}

func addOrderMesh(scene *core.Node, geom geometry.IGeometry, mat material.IMaterial, x, y, z float32) {
	mesh := graphic.NewMesh(geom, mat)
	mesh.SetPosition(x, y, z)
	scene.Add(mesh)
}

func groupFixtureItems() []selectionFixtureEntity {
	return []selectionFixtureEntity{
		{Name: "group-1", World: math32.Vector3{X: -80, Y: 18, Z: 80}, Item: selectionItem(1, 1, litinput.SelectUnit, 0, false)},
		{Name: "group-dead-2", World: math32.Vector3{X: 20, Y: 18, Z: 80}, Item: selectionItem(2, 1, litinput.SelectUnit, 0, false)},
		{Name: "group-3", World: math32.Vector3{X: 120, Y: 18, Z: 80}, Item: selectionItem(3, 1, litinput.SelectUnit, 0, false)},
		{Name: "group-dead-4", World: math32.Vector3{X: 220, Y: 18, Z: 80}, Item: selectionItem(4, 1, litinput.SelectUnit, 0, false)},
		{Name: "group-5", World: math32.Vector3{X: 320, Y: 18, Z: 80}, Item: selectionItem(5, 1, litinput.SelectUnit, 0, false)},
		{Name: "add-6", World: math32.Vector3{X: 520, Y: 18, Z: 80}, Item: selectionItem(6, 1, litinput.SelectUnit, 0, false)},
		{Name: "add-7", World: math32.Vector3{X: 660, Y: 18, Z: 80}, Item: selectionItem(7, 1, litinput.SelectUnit, 0, false)},
		{Name: "add-8", World: math32.Vector3{X: 800, Y: 18, Z: 80}, Item: selectionItem(8, 1, litinput.SelectUnit, 0, false)},
	}
}

func groupFixtureLiveAfterDeaths(items []selectionFixtureEntity) []selectionFixtureEntity {
	out := make([]selectionFixtureEntity, 0, 6)
	for _, item := range items {
		switch item.Item.ID {
		case 2, 4:
			continue
		default:
			out = append(out, item)
		}
	}
	return out
}

func groupLiveEntities(items []selectionFixtureEntity) []litinput.GroupEntity {
	out := make([]litinput.GroupEntity, 0, len(items))
	for _, item := range items {
		out = append(out, litinput.GroupEntity{ID: item.Item.ID, X: item.World.X, Z: item.World.Z})
	}
	return out
}

func groupSelection(ids ...uint32) litinput.Selection {
	var s litinput.Selection
	for _, id := range ids {
		if s.Count >= sim.MaxCommandUnits {
			break
		}
		s.IDs[s.Count] = sim.EntityID(id)
		s.Count++
	}
	return s
}

func groupSelectionIDs(ids ...sim.EntityID) litinput.Selection {
	var s litinput.Selection
	for _, id := range ids {
		if s.Count >= sim.MaxCommandUnits {
			break
		}
		s.IDs[s.Count] = id
		s.Count++
	}
	return s
}

func groupCase(name, gesture string, groups litinput.ControlGroups, res litinput.GroupResult, expectedSelection, expectedGroup []sim.EntityID, expectedPruned uint8, expectedCenter bool, expectedX, expectedZ float32) groupCaseDump {
	groupIDs, _ := groups.IDs(int(res.Group))
	out := groupCaseDump{
		Name:                  name,
		Gesture:               gesture,
		Group:                 res.Group,
		GroupIDs:              selectionCommandIDs(groupIDs),
		ExpectedGroupIDs:      selectionCommandIDs(expectedGroup),
		Selection:             selectionCommandIDs(selectionIDs(res.Selection)),
		Expected:              selectionCommandIDs(expectedSelection),
		Pruned:                res.Pruned,
		ExpectedPruned:        expectedPruned,
		CenterRequested:       res.CenterRequested,
		ExpectedCenter:        expectedCenter,
		CenterX:               res.CenterX,
		CenterZ:               res.CenterZ,
		ExpectedCenterX:       expectedX,
		ExpectedCenterZ:       expectedZ,
		CommandRecordsEmitted: res.CommandRecordsEmitted,
	}
	out.OK = sameEntityIDs(selectionIDs(res.Selection), expectedSelection) &&
		sameEntityIDs(groupIDs, expectedGroup) &&
		res.Pruned == expectedPruned &&
		res.CenterRequested == expectedCenter &&
		res.CommandRecordsEmitted == 0
	if expectedCenter {
		out.OK = out.OK && res.CenterX == expectedX && res.CenterZ == expectedZ
	}
	return out
}

func buildCanvasHUD(scene *core.Node, res resolutionFlag, uiScale float64, resizeFrom resolutionFlag, sceneName, cardScenario, resbarScenario, campaignScenario, localeTag, keymapPath string, localeTable *litlocale.Table, labels lithud.HUDStrings) (canvasDump, error) {
	canvas, err := lithud.NewCanvas(res.W, res.H, uiScale)
	if err != nil {
		return canvasDump{}, err
	}
	if strings.EqualFold(strings.TrimSpace(sceneName), "campaign-menu") || strings.TrimSpace(campaignScenario) != "" {
		return buildCampaignMenuHUD(scene, canvas, campaignScenario, localeTag, localeTable)
	}
	hud := lithud.NewDefaultHUDWithStrings(canvas, labels)
	after := canvasSnapshotFor(canvas, hud.Widgets())
	scenarios := hud.RunFSVScenarios()
	dump := canvasDump{
		Mode:  "hud-full",
		After: after,
		HUD: hudRuntimeDump{
			AtlasPath:              lithud.DefaultAtlasPath,
			Locale:                 localeTag,
			WidgetPanels:           hud.PanelDrawCalls(),
			Labels:                 hud.LabelDrawCalls(),
			ExpectedGUIDrawCalls:   hud.ExpectedGUIDrawCalls(),
			DrawCallBudget:         lithud.DefaultHUDDrawCallCap,
			WorstUpdateMicrosFrame: worstUpdateMicrosFrame(scenarios),
			UpdateScenarios:        scenarios,
		},
	}
	var card *lithud.CommandCard
	if sceneName == "basecamp" || cardScenario != "" {
		cardDump, displayCard, err := buildCommandCardFSV(localeTable, cardScenario, keymapPath)
		if err != nil {
			return canvasDump{}, err
		}
		dump.CommandCard = cardDump
		card = displayCard
	}
	if sceneName == "basecamp" || resbarScenario != "" {
		resourceDump, err := buildResourceBarFSV(&hud, resbarScenario)
		if err != nil {
			return canvasDump{}, err
		}
		dump.ResourceBar = resourceDump
	}
	if resizeFrom.set {
		beforeCanvas, err := lithud.NewCanvas(resizeFrom.W, resizeFrom.H, uiScale)
		if err != nil {
			return canvasDump{}, fmt.Errorf("resize-from: %w", err)
		}
		beforeHUD := lithud.NewDefaultHUDWithStrings(beforeCanvas, labels)
		before := canvasSnapshotFor(beforeCanvas, beforeHUD.Widgets())
		dump.Before = &before
	}
	dump.OK, dump.Errors = validateCanvasSnapshot(after)
	atlasTex, err := texture.NewTexture2DFromImage(lithud.DefaultAtlasPath)
	if err != nil {
		return canvasDump{}, fmt.Errorf("ui atlas: %w", err)
	}
	drawCanvasHUD(scene, after, &hud, card, atlasTex, dump.OK)
	return dump, nil
}

func buildCampaignMenuHUD(scene *core.Node, canvas lithud.Canvas, scenario, localeTag string, localeTable *litlocale.Table) (canvasDump, error) {
	runtimeDump, err := buildCampaignMenuRuntime(canvas, scenario, localeTag, localeTable)
	after := canvasSnapshotFor(canvas, runtimeDump.Layout.Widgets)
	dump := canvasDump{
		Mode:  "campaign-menu",
		After: after,
		HUD: hudRuntimeDump{
			Locale:               localeTag,
			WidgetPanels:         len(runtimeDump.Layout.Widgets),
			Labels:               len(runtimeDump.Layout.Labels),
			ExpectedGUIDrawCalls: runtimeDump.Layout.ExpectedDrawCalls,
			DrawCallBudget:       runtimeDump.Layout.ExpectedDrawCalls,
		},
		CampaignMenu: runtimeDump,
		OK:           runtimeDump.OK,
		Errors:       append([]string{}, runtimeDump.Errors...),
	}
	drawCampaignMenu(scene, runtimeDump.Layout, runtimeDump.OK)
	return dump, err
}

const renderDemoCampaignTOML = `
id = "vigil-render"
title = "Vigil Render Campaign"
faction = "The Vigil"

[[mission]]
id = "m1"
title = "Kindle the Gate"
summary = "Secure the first beacon."
archive = "worlds/m1.litdworld"

[[mission]]
id = "m2"
title = "Hold the Dawn"
summary = "Carry the hero into the counterattack."
archive = "worlds/m2.litdworld"
requires = ["m1"]
`

func buildCampaignMenuRuntime(canvas lithud.Canvas, scenario, localeTag string, localeTable *litlocale.Table) (*campaignMenuRuntimeDump, error) {
	scenario = strings.ToLower(strings.TrimSpace(scenario))
	if scenario == "" {
		scenario = "fresh"
	}
	def, err := litcampaign.ReadDefinition("renderdemo-campaign.toml", []byte(renderDemoCampaignTOML))
	if err != nil {
		return nil, err
	}
	g, err := api.NewGame(api.GameOptions{})
	if err != nil {
		return nil, err
	}
	labels := lithud.CampaignMenuStringsFromLocale(localeTable)
	archives := renderDemoCampaignArchives(true)
	dump := &campaignMenuRuntimeDump{Scenario: scenario, Locale: localeTag, OK: true}
	var layout lithud.CampaignMenuLayout
	switch scenario {
	case "campaign-select":
		catalog, err := litcampaign.BuildCatalogView([]litcampaign.Definition{def}, g.Storage(), def.ID)
		if err != nil {
			return nil, err
		}
		store, err := litcampaign.SnapshotStore(def, g.Storage())
		if err != nil {
			return nil, err
		}
		layout = lithud.NewCampaignSelectLayout(canvas, catalog, labels)
		dump.Catalog = &catalog
		dump.AfterStore = store
	case "fresh":
		view, err := litcampaign.BuildMissionView(def, g.Storage(), archives, "")
		if err != nil {
			return nil, err
		}
		store, err := litcampaign.SnapshotStore(def, g.Storage())
		if err != nil {
			return nil, err
		}
		layout = lithud.NewMissionSelectLayout(canvas, view, labels)
		dump.View = &view
		dump.AfterStore = store
	case "unlocked":
		before, err := litcampaign.SnapshotStore(def, g.Storage())
		if err != nil {
			return nil, err
		}
		if err := litcampaign.CompleteMission(g.Storage(), def, "m1", renderDemoCarryOver("Ser Caldus", 4, "Ember Ward", "Dawnwater Flask")); err != nil {
			return nil, err
		}
		view, err := litcampaign.BuildMissionView(def, g.Storage(), archives, "")
		if err != nil {
			return nil, err
		}
		after, err := litcampaign.SnapshotStore(def, g.Storage())
		if err != nil {
			return nil, err
		}
		layout = lithud.NewMissionSelectLayout(canvas, view, labels)
		dump.BeforeStore = &before
		dump.AfterStore = after
		dump.View = &view
	case "save-load":
		g.Storage().SetString("campaign:"+def.ID, "mission:m1:checkpoint", "inside-the-gate")
		before, err := litcampaign.SnapshotStore(def, g.Storage())
		if err != nil {
			return nil, err
		}
		var buf bytes.Buffer
		if err := g.Storage().Save(&buf); err != nil {
			return nil, err
		}
		reloaded, err := api.NewGame(api.GameOptions{})
		if err != nil {
			return nil, err
		}
		if err := reloaded.Storage().Load(bytes.NewReader(buf.Bytes())); err != nil {
			return nil, err
		}
		dump.Checkpoint, dump.CheckpointRead = reloaded.Storage().GetString("campaign:"+def.ID, "mission:m1:checkpoint")
		if err := litcampaign.CompleteMission(reloaded.Storage(), def, "m1", renderDemoCarryOver("Mira Vale", 2, "Signal Charm")); err != nil {
			return nil, err
		}
		view, err := litcampaign.BuildMissionView(def, reloaded.Storage(), archives, "")
		if err != nil {
			return nil, err
		}
		after, err := litcampaign.SnapshotStore(def, reloaded.Storage())
		if err != nil {
			return nil, err
		}
		layout = lithud.NewMissionSelectLayout(canvas, view, labels)
		dump.BeforeStore = &before
		dump.AfterStore = after
		dump.View = &view
	case "missing-archive":
		view, err := litcampaign.BuildMissionView(def, g.Storage(), renderDemoCampaignArchives(false), "")
		if err != nil {
			return nil, err
		}
		store, err := litcampaign.SnapshotStore(def, g.Storage())
		if err != nil {
			return nil, err
		}
		layout = lithud.NewMissionSelectLayout(canvas, view, labels)
		dump.AfterStore = store
		dump.View = &view
	default:
		return nil, fmt.Errorf("campaign-menu: unknown scenario %q", scenario)
	}
	dump.Layout = layout
	dump.Screen = layout.Screen
	if len(layout.Issues) > 0 {
		dump.OK = false
		for _, issue := range layout.Issues {
			dump.Errors = append(dump.Errors, fmt.Sprintf("%s %s: %s", issue.Widget, issue.Rule, issue.Msg))
		}
	}
	if scenario == "save-load" && (!dump.CheckpointRead || dump.Checkpoint != "inside-the-gate") {
		dump.OK = false
		dump.Errors = append(dump.Errors, fmt.Sprintf("checkpoint read = (%q,%v), want inside-the-gate,true", dump.Checkpoint, dump.CheckpointRead))
	}
	return dump, nil
}

func renderDemoCampaignArchives(complete bool) fstest.MapFS {
	if complete {
		return fstest.MapFS{
			"worlds/m1.litdworld": {Data: []byte("m1")},
			"worlds/m2.litdworld": {Data: []byte("m2")},
		}
	}
	return fstest.MapFS{
		"worlds/m2.litdworld": {Data: []byte("m2")},
	}
}

func renderDemoCarryOver(name string, level int, items ...string) litcampaign.CarryOver {
	return litcampaign.CarryOver{
		MissionID: "m2",
		Heroes: []litcampaign.HeroCarryOver{{
			Name:  name,
			Level: level,
			Items: append([]string{}, items...),
		}},
	}
}

func buildResourceBarFSV(hud *lithud.DefaultHUD, scenario string) (*resourceBarRuntimeDump, error) {
	if scenario == "" {
		scenario = "initial"
	}
	names := []string{"initial", "after-spend", "foodcap", "insufficient", "large"}
	dump := &resourceBarRuntimeDump{Scenario: scenario}
	for _, name := range names {
		state, ok := renderDemoResourceScenarioState(name)
		if !ok {
			return nil, fmt.Errorf("resourcebar: unknown scenario %q", name)
		}
		dump.Cases = append(dump.Cases, snapshotResourceBarCase(hud.Labels, name, state))
	}
	state, ok := renderDemoResourceScenarioState(scenario)
	if !ok {
		return nil, fmt.Errorf("resourcebar: unknown scenario %q", scenario)
	}
	dump.Current = applyResourceBarCase(hud, scenario, state)
	dump.Feedback = hud.ResourceBar.FeedbackEvents()
	return dump, nil
}

func snapshotResourceBarCase(labels lithud.HUDStrings, name string, state lithud.HUDState) resourceBarCaseDump {
	var text lithud.TextBuffer
	bar := lithud.NewResourceBar(&text, lithud.ResourceBarStringsFromHUD(labels))
	var feedback []lithud.ResourceFeedback
	if name == "insufficient" {
		feedback = append(feedback, bar.InsufficientGold(12, state.Gold))
	}
	update := bar.Update(lithud.ResourceBarState{Gold: state.Gold, Lumber: state.Lumber, FoodUsed: state.FoodUsed, FoodCap: state.FoodCap, Upkeep: state.Upkeep, Tick: resourceBarTickFor(name)})
	return resourceBarCaseDump{Name: name, Sim: resourceValuesFor(state), Displayed: text.String(), Update: update, Feedback: feedback}
}

func applyResourceBarCase(hud *lithud.DefaultHUD, name string, state lithud.HUDState) resourceBarCaseDump {
	hud.Update(state)
	var feedback []lithud.ResourceFeedback
	if name == "insufficient" {
		feedback = append(feedback, hud.ResourceBar.InsufficientGold(12, state.Gold))
	}
	update := hud.ResourceBar.Update(lithud.ResourceBarState{Gold: state.Gold, Lumber: state.Lumber, FoodUsed: state.FoodUsed, FoodCap: state.FoodCap, Upkeep: state.Upkeep, Tick: resourceBarTickFor(name)})
	return resourceBarCaseDump{Name: name, Sim: resourceValuesFor(state), Displayed: hud.Resource.String(), Update: update, Feedback: feedback}
}

func resourceBarTickFor(name string) uint32 {
	if name == "insufficient" {
		return 12
	}
	if name == "large" {
		return 60
	}
	return 0
}

func renderDemoResourceScenarioState(name string) (lithud.HUDState, bool) {
	state := lithud.DefaultHUDState()
	switch name {
	case "initial":
		return state, true
	case "after-spend":
		state.Gold -= 135
		state.FoodUsed++
		return state, true
	case "foodcap":
		state.Gold = 999
		state.Lumber = 888
		state.FoodUsed = 100
		state.FoodCap = 100
		state.Upkeep = 2
		return state, true
	case "insufficient":
		return state, true
	case "large":
		state.Gold = 9999
		state.Lumber = 12000
		state.FoodUsed = 99
		state.FoodCap = 100
		state.Upkeep = 3
		return state, true
	default:
		return lithud.HUDState{}, false
	}
}

func resourceValuesFor(state lithud.HUDState) resourceBarValues {
	return resourceBarValues{
		Gold:     state.Gold,
		Lumber:   state.Lumber,
		FoodUsed: state.FoodUsed,
		FoodCap:  state.FoodCap,
		Upkeep:   state.Upkeep,
	}
}

func buildCommandCardFSV(localeTable *litlocale.Table, scenario, keymapPath string) (*commandCardRuntimeDump, *lithud.CommandCard, error) {
	if scenario == "" {
		scenario = "unit"
	}
	table, err := lithud.LoadCommandCardTable(os.DirFS("data"))
	if err != nil {
		return nil, nil, err
	}
	keymap, keymapLabel, err := renderDemoKeymap(keymapPath)
	if err != nil {
		return nil, nil, err
	}
	applyCommandCardKeymap(table, keymap)
	states := []struct {
		name  string
		state lithud.CommandCardState
	}{
		{name: "unit", state: renderDemoCardUnitState()},
		{name: "building", state: renderDemoCardBuildingState()},
		{name: "subgroup", state: renderDemoCardSubgroupState()},
		{name: "enemy", state: renderDemoCardEnemyState()},
		{name: "cooldown", state: renderDemoCardCooldownState()},
		{name: "empty", state: renderDemoCardEmptyState()},
	}
	dump := &commandCardRuntimeDump{TablePath: table.Path, KeymapPath: keymapLabel, KeymapProfile: keymap.Profile, Scenario: scenario}
	for _, entry := range states {
		card := lithud.NewCommandCard(table, localeTable)
		dump.Cases = append(dump.Cases, snapshotCommandCardCase(entry.name, &card, entry.state))
	}

	emitter := lithud.NewCommandCard(table, localeTable)
	emitter.Refresh(renderDemoCardUnitState())
	click := emitter.ClickSlot(0, false)
	dump.Clicks = append(dump.Clicks, click)
	if click.Accepted && click.PendingTarget {
		if rec, ok := emitter.ConfirmTarget(fixed.Vec2{X: fixed.FromInt(320), Y: fixed.FromInt(480)}, 0, false); ok {
			dump.Emitted = append(dump.Emitted, commandRecordDumpFor(rec))
		}
	}
	disabled := lithud.NewCommandCard(table, localeTable)
	disabled.Refresh(renderDemoCardCooldownState())
	dump.Clicks = append(dump.Clicks, disabled.ClickSlot(1, false))
	keyEmitter := lithud.NewCommandCard(table, localeTable)
	keyEmitter.Refresh(renderDemoCardUnitState())
	slot0Key := table.GridHotkeys[0]
	dump.KeyPresses = append(dump.KeyPresses, commandCardKeyPress(keyEmitter, keymap, slot0Key))
	if slot0Key != "Q" {
		dump.KeyPresses = append(dump.KeyPresses, commandCardKeyPress(keyEmitter, keymap, "Q"))
	}

	currentState, ok := renderDemoCardScenarioState(scenario)
	if !ok {
		return nil, nil, fmt.Errorf("command-card: unknown scenario %q", scenario)
	}
	display := lithud.NewCommandCard(table, localeTable)
	dump.Current = snapshotCommandCardCase(scenario, &display, currentState)
	return dump, &display, nil
}

func renderDemoKeymap(path string) (*litinput.Keymap, string, error) {
	base, err := litinput.LoadKeymap(os.DirFS("data"), litinput.DefaultKeymapPath)
	if err != nil {
		return nil, "", err
	}
	if strings.TrimSpace(path) == "" {
		return base, litinput.DefaultKeymapPath, nil
	}
	override, err := litinput.LoadKeymapFile(path)
	if err != nil {
		return nil, "", err
	}
	merged, err := base.Overlay(override)
	if err != nil {
		return nil, "", err
	}
	return merged, path, nil
}

func applyCommandCardKeymap(table *lithud.CommandCardTable, keymap *litinput.Keymap) {
	keys := keymap.CommandCardHotkeys(litinput.ContextGame)
	for i := range keys {
		if keys[i] != "" {
			table.GridHotkeys[i] = keys[i]
		}
	}
}

func commandCardKeyPress(card lithud.CommandCard, keymap *litinput.Keymap, key string) commandCardKeyPressDump {
	out := commandCardKeyPressDump{Key: key}
	binding, ok := keymap.Resolve(litinput.ContextGame, litinput.Key(key))
	if !ok {
		out.Reason = "unbound"
		return out
	}
	out.Action = binding.Action
	slot, ok := litinput.CommandCardSlot(binding.Action)
	if !ok {
		out.Reason = "not command-card slot"
		return out
	}
	out.Slot = slot
	click := card.ClickSlot(slot, false)
	out.Accepted = click.Accepted
	out.PendingTarget = click.PendingTarget
	out.Reason = click.Reason
	if click.Accepted && click.PendingTarget {
		if rec, ok := card.ConfirmTarget(fixed.Vec2{X: fixed.FromInt(320), Y: fixed.FromInt(480)}, 0, false); ok {
			dump := commandRecordDumpFor(rec)
			out.Emitted = &dump
		}
	}
	return out
}

func snapshotCommandCardCase(name string, card *lithud.CommandCard, state lithud.CommandCardState) commandCardCaseDump {
	update := card.Refresh(state)
	return commandCardCaseDump{
		Name:           name,
		Selection:      state.SelectionLabel,
		ActiveSubgroup: card.ActiveSubgroup,
		Visible:        card.Visible,
		Summary:        card.Summary.String(),
		Update:         update,
		Slots:          visibleCommandCardSlots(card),
	}
}

func visibleCommandCardSlots(card *lithud.CommandCard) []lithud.CommandCardSlotState {
	out := make([]lithud.CommandCardSlotState, 0, lithud.CommandCardSlots)
	for _, slot := range card.Slots {
		if slot.Visible {
			out = append(out, slot)
		}
	}
	return out
}

func commandRecordDumpFor(r sim.CommandRecord) commandRecordDump {
	out := commandRecordDump{
		Version:   r.Version,
		Player:    r.Player,
		Seq:       r.Seq,
		Opcode:    r.Opcode,
		Flags:     r.Flags,
		UnitCount: r.UnitCount,
		Target:    uint32(r.Target),
		PointX:    int64(r.Point.X),
		PointY:    int64(r.Point.Y),
		Data:      r.Data,
	}
	out.Units = make([]uint32, 0, r.UnitCount)
	for i := uint8(0); i < r.UnitCount; i++ {
		out.Units = append(out.Units, uint32(r.Units[i]))
	}
	return out
}

func renderDemoCardScenarioState(name string) (lithud.CommandCardState, bool) {
	switch name {
	case "unit":
		return renderDemoCardUnitState(), true
	case "building":
		return renderDemoCardBuildingState(), true
	case "subgroup":
		return renderDemoCardSubgroupState(), true
	case "enemy":
		return renderDemoCardEnemyState(), true
	case "cooldown":
		return renderDemoCardCooldownState(), true
	case "empty":
		return renderDemoCardEmptyState(), true
	default:
		return lithud.CommandCardState{}, false
	}
}

func renderDemoCardUnitState() lithud.CommandCardState {
	var state lithud.CommandCardState
	state.Player = 0
	state.OwnSelection = true
	state.SelectionLabel = "footman"
	state.Subgroups[0] = "footman"
	state.SubgroupCount = 1
	state.UnitCount = 2
	state.Units[0], state.Units[1] = 101, 102
	state.Gold, state.Lumber = 725, 240
	return state
}

func renderDemoCardBuildingState() lithud.CommandCardState {
	var state lithud.CommandCardState
	state.Player = 0
	state.OwnSelection = true
	state.SelectionLabel = "barracks"
	state.Subgroups[0] = "barracks"
	state.SubgroupCount = 1
	state.UnitCount = 1
	state.Units[0] = 201
	state.Gold, state.Lumber = 725, 240
	return state
}

func renderDemoCardSubgroupState() lithud.CommandCardState {
	state := renderDemoCardUnitState()
	state.SelectionLabel = "mixed"
	state.Subgroups[1] = "barracks"
	state.SubgroupCount = 2
	state.UnitCount = 3
	state.Units[2] = 201
	lithud.CycleCommandSubgroup(&state)
	return state
}

func renderDemoCardEnemyState() lithud.CommandCardState {
	state := renderDemoCardUnitState()
	state.OwnSelection = false
	state.SelectionLabel = "enemy-footman"
	return state
}

func renderDemoCardCooldownState() lithud.CommandCardState {
	state := renderDemoCardBuildingState()
	state.SelectionLabel = "barracks-low"
	state.Gold = 100
	state.Lumber = 0
	state.Cooldown[0] = 5
	return state
}

func renderDemoCardEmptyState() lithud.CommandCardState {
	state := renderDemoCardUnitState()
	state.SelectionLabel = "empty"
	state.UnitCount = 0
	return state
}

func canvasSnapshotFor(canvas lithud.Canvas, widgets []lithud.Widget) canvasSnapshot {
	rects := make([]canvasRegionDump, 0, len(widgets))
	for _, widget := range widgets {
		rects = append(rects, canvasRegionDump{
			Name:   widget.Name,
			Anchor: widget.Anchor.String(),
			Kind:   widget.Kind.String(),
			Parent: widget.Parent,
			Atlas:  widget.AtlasRegion,
			CellsX: widget.CellsX,
			CellsY: widget.CellsY,
			Ref:    widget.Ref,
			Rect:   widget.Rect,
		})
	}
	return canvasSnapshot{
		Width:   canvas.Width,
		Height:  canvas.Height,
		UIScale: canvas.UIScale,
		Scale:   canvas.Scale,
		Rects:   rects,
	}
}

func validateCanvasSnapshot(s canvasSnapshot) (bool, []string) {
	var errs []string
	for i, r := range s.Rects {
		if !r.Rect.Inside(s.Width, s.Height) {
			errs = append(errs, fmt.Sprintf("%s offscreen %+v", r.Name, r.Rect))
		}
		if r.Parent != "" {
			parent, ok := snapshotRect(s.Rects, r.Parent)
			if !ok || !r.Rect.InsideRect(parent) {
				errs = append(errs, fmt.Sprintf("%s outside parent %s %+v", r.Name, r.Parent, r.Rect))
			}
			continue
		}
		for j := 0; j < i; j++ {
			if s.Rects[j].Parent == "" && r.Rect.Overlaps(s.Rects[j].Rect) {
				errs = append(errs, fmt.Sprintf("%s overlaps %s", r.Name, s.Rects[j].Name))
			}
		}
	}
	return len(errs) == 0, errs
}

func snapshotRect(rects []canvasRegionDump, name string) (lithud.Rect, bool) {
	for _, r := range rects {
		if r.Name == name {
			return r.Rect, true
		}
	}
	return lithud.Rect{}, false
}

func drawCanvasHUD(scene *core.Node, snap canvasSnapshot, hud *lithud.DefaultHUD, card *lithud.CommandCard, atlasTex *texture.Texture2D, ok bool) {
	for _, region := range snap.Rects {
		rect := region.Rect
		panel := gui.NewPanel(float32(rect.W), float32(rect.H))
		color := hudColor(region)
		panel.SetColor4(&color)
		panel.Material().AddTexture(atlasTex)
		panel.SetPosition(float32(rect.X), float32(rect.Y))
		scene.Add(panel)
	}

	for _, region := range snap.Rects {
		if region.Parent != "" {
			continue
		}
		rect := region.Rect
		label := gui.NewLabel(hudLabel(region.Name, hud, card, ok))
		y := rect.Y + 22
		if rect.H < 34 {
			y = rect.Y + rect.H - 12
		}
		label.SetPosition(float32(rect.X+6), float32(y))
		scene.Add(label)
	}
}

func drawCampaignMenu(scene *core.Node, layout lithud.CampaignMenuLayout, ok bool) {
	for _, widget := range layout.Widgets {
		rect := widget.Rect
		panel := gui.NewPanel(float32(rect.W), float32(rect.H))
		color := campaignMenuColor(widget.Name, ok)
		panel.SetColor4(&color)
		panel.SetPosition(float32(rect.X), float32(rect.Y))
		scene.Add(panel)
	}
	for _, entry := range layout.Labels {
		label := gui.NewLabel(entry.Text)
		fg := math32.Color4{R: 0.88, G: 0.91, B: 0.88, A: 1}
		label.SetColor4(&fg)
		label.SetPosition(float32(entry.Rect.X), float32(entry.Rect.Y))
		scene.Add(label)
	}
}

func campaignMenuColor(name string, ok bool) math32.Color4 {
	if !ok && name == "campaign-header" {
		return math32.Color4{R: 0.42, G: 0.10, B: 0.10, A: 0.94}
	}
	switch name {
	case "campaign-header":
		return math32.Color4{R: 0.09, G: 0.14, B: 0.19, A: 0.96}
	case "campaign-list", "mission-list":
		return math32.Color4{R: 0.12, G: 0.16, B: 0.20, A: 0.94}
	case "carry-over":
		return math32.Color4{R: 0.18, G: 0.16, B: 0.10, A: 0.94}
	default:
		return math32.Color4{R: 0.15, G: 0.19, B: 0.23, A: 0.94}
	}
}

func hudColor(region canvasRegionDump) math32.Color4 {
	switch region.Kind {
	case "icon-grid":
		return math32.Color4{R: 0.20, G: 0.24, B: 0.34, A: 0.92}
	case "progress-bar":
		if region.Name == "mana-bar" {
			return math32.Color4{R: 0.16, G: 0.30, B: 0.62, A: 0.95}
		}
		return math32.Color4{R: 0.18, G: 0.56, B: 0.24, A: 0.95}
	default:
		switch region.Name {
		case "resource-bar":
			return math32.Color4{R: 0.34, G: 0.23, B: 0.50, A: 0.92}
		case "minimap":
			return math32.Color4{R: 0.18, G: 0.42, B: 0.27, A: 0.92}
		case "portrait":
			return math32.Color4{R: 0.36, G: 0.29, B: 0.16, A: 0.92}
		case "info-panel":
			return math32.Color4{R: 0.17, G: 0.33, B: 0.46, A: 0.92}
		case "command-card":
			return math32.Color4{R: 0.42, G: 0.18, B: 0.18, A: 0.92}
		default:
			return math32.Color4{R: 0.12, G: 0.30, B: 0.52, A: 0.92}
		}
	}
}

func hudLabel(name string, hud *lithud.DefaultHUD, card *lithud.CommandCard, ok bool) string {
	switch name {
	case "resource-bar":
		return hud.Resource.String()
	case "portrait":
		return hud.Vitals.String()
	case "info-panel":
		return hud.Selection.String()
	case "command-card":
		if card != nil {
			return card.Summary.String()
		}
		return hud.Queue.String()
	case "control-groups":
		return hud.Groups.String()
	case "menu-cluster":
		if ok {
			return hud.Labels.MenuOKTrue
		}
		return hud.Labels.MenuOKFalse
	case "idle-worker":
		return hud.Labels.IdleWorker
	case "minimap":
		return hud.Labels.Minimap
	default:
		return name
	}
}

func worstUpdateMicrosFrame(s lithud.FSVScenarios) float64 {
	worst := perFrameMicros(s.Static100)
	for _, v := range []float64{perFrameMicros(s.ResourceChurn), perFrameMicros(s.SelectionChurn)} {
		if v > worst {
			worst = v
		}
	}
	return worst
}

func perFrameMicros(s lithud.ScenarioStats) float64 {
	if s.Frames == 0 {
		return 0
	}
	return float64(s.UpdateMicros) / float64(s.Frames)
}

func (d *canvasDump) recordFrameStats(stats litrender.FrameStats) {
	d.HUD.ActualGUIDrawCalls = stats.GUIDrawCalls
	d.HUD.GUIStateChanges = stats.GUIStates
	if stats.GUIDrawCalls > d.HUD.DrawCallBudget {
		d.OK = false
		d.Errors = append(d.Errors, fmt.Sprintf("gui draw calls %d exceed budget %d", stats.GUIDrawCalls, d.HUD.DrawCallBudget))
	}
	if d.HUD.WorstUpdateMicrosFrame > 1000 {
		d.OK = false
		d.Errors = append(d.Errors, fmt.Sprintf("hud update %.3fus/frame exceeds 1000us", d.HUD.WorstUpdateMicrosFrame))
	}
}

func expectedStats(visible, culled, opaqueDraws, transparentDraws, opaqueStates, transparentStates int) litrender.FrameStats {
	worldDraws := opaqueDraws + transparentDraws
	return litrender.FrameStats{
		GraphicMaterials:     visible,
		Lights:               worldDraws * 2,
		Panels:               1,
		Others:               1,
		VisibleGraphics:      visible,
		CulledGraphics:       culled,
		DrawCalls:            worldDraws + 1,
		OpaqueDrawCalls:      opaqueDraws,
		TransparentDrawCalls: transparentDraws,
		GUIDrawCalls:         1,
		StateChanges:         opaqueStates + transparentStates + 1,
		OpaqueStates:         opaqueStates,
		TransparentStates:    transparentStates,
		GUIStates:            1,
	}
}

func screenshot(a *app.Application, path string) error {
	w, h := a.GetFramebufferSize()
	data := a.Gls().ReadPixels(0, 0, w, h, gls.RGBA, gls.UNSIGNED_BYTE)
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	row := w * 4
	for y := 0; y < h; y++ {
		copy(img.Pix[y*img.Stride:y*img.Stride+row], data[(h-1-y)*row:(h-y)*row])
	}
	if dir := filepath.Dir(path); dir != "." {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return err
		}
	}
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	return png.Encode(f, img)
}

func writeJSONFile(path string, v interface{}) error {
	if dir := filepath.Dir(path); dir != "." {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return err
		}
	}
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	enc := json.NewEncoder(f)
	enc.SetIndent("", "  ")
	return enc.Encode(v)
}
