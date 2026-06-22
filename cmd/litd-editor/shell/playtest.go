package shell

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldpack"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

type PlaytestOptions struct {
	TempRoot  string
	Command   []string
	Dir       string
	ShotPath  string
	Timeout   time.Duration
	KillAfter time.Duration
	KeepTemp  bool
}

type PlaytestSnapshot struct {
	ArchivePath     string   `json:"archivePath,omitempty"`
	ShotPath        string   `json:"shotPath,omitempty"`
	TempDir         string   `json:"tempDir,omitempty"`
	TempBefore      []string `json:"tempBefore,omitempty"`
	TempAfter       []string `json:"tempAfter,omitempty"`
	StateHashBefore string   `json:"stateHashBefore,omitempty"`
	StateHashAfter  string   `json:"stateHashAfter,omitempty"`
	Command         []string `json:"command,omitempty"`
	ExitCode        int      `json:"exitCode"`
	Killed          bool     `json:"killed,omitempty"`
	Error           string   `json:"error,omitempty"`
	Stdout          string   `json:"stdout,omitempty"`
	Stderr          string   `json:"stderr,omitempty"`
	ManifestTitle   string   `json:"manifestTitle,omitempty"`
	ManifestFiles   []string `json:"manifestFiles,omitempty"`
}

func (a *App) Playtest(opts PlaytestOptions) (rec PlaytestSnapshot, err error) {
	beforeHash := a.SimRelevantHash()
	rec = PlaytestSnapshot{StateHashBefore: beforeHash, ExitCode: -1}
	tempDir, archivePath, shotPath, err := a.buildPlaytestArchive(opts)
	if err != nil {
		rec.Error = err.Error()
		rec.StateHashAfter = a.SimRelevantHash()
		a.errText = err.Error()
		a.status = a.errText
		a.playtest = rec
		return rec, err
	}
	rec.TempDir = tempDir
	rec.ArchivePath = archivePath
	rec.ShotPath = shotPath
	rec.TempBefore = listDirNames(filepath.Dir(tempDir))
	defer func() {
		if !opts.KeepTemp {
			_ = os.RemoveAll(tempDir)
		}
		rec.TempAfter = listDirNames(filepath.Dir(tempDir))
		rec.StateHashAfter = a.SimRelevantHash()
		a.playtest = rec
	}()

	opened, err := worldarchive.Open(archivePath, EditorEngineVersion())
	if err != nil {
		rec.Error = err.Error()
		a.errText = err.Error()
		a.status = a.errText
		return rec, err
	}
	rec.ManifestTitle = opened.Manifest.Title
	for rel := range opened.Manifest.Files {
		rec.ManifestFiles = append(rec.ManifestFiles, rel)
	}
	sort.Strings(rec.ManifestFiles)
	opened.Close()

	cmdline := playtestCommand(opts.Command, archivePath, shotPath)
	rec.Command = append([]string(nil), cmdline...)
	stdout, stderr, exitCode, killed, runErr := runPlaytestProcess(cmdline, opts.Dir, opts.Timeout, opts.KillAfter)
	rec.Stdout = stdout
	rec.Stderr = stderr
	rec.ExitCode = exitCode
	rec.Killed = killed
	if runErr != nil {
		rec.Error = runErr.Error()
		a.errText = runErr.Error()
		a.status = a.errText
		return rec, runErr
	}
	a.errText = ""
	a.status = fmt.Sprintf("Playtest exited: %d", exitCode)
	return rec, nil
}

func (a *App) buildPlaytestArchive(opts PlaytestOptions) (tempDir, archivePath, shotPath string, err error) {
	if a.world == nil {
		return "", "", "", errors.New("editor playtest: no project loaded")
	}
	if a.archiveReadOnly {
		return "", "", "", errors.New("editor playtest: source-form project required")
	}
	if len(a.world.Terrain.StartLocations) == 0 {
		return "", "", "", errors.New("editor playtest: at least one start location is required")
	}
	base := sanitizeWorldID(a.world.Metadata.ID)
	if base == "" {
		base = "world"
	}
	if opts.TempRoot != "" {
		if err := os.MkdirAll(opts.TempRoot, 0o755); err != nil {
			return "", "", "", err
		}
	}
	tempDir, err = os.MkdirTemp(opts.TempRoot, "litd-editor-playtest-"+base+"-")
	if err != nil {
		return "", "", "", err
	}
	defer func() {
		if err != nil {
			_ = os.RemoveAll(tempDir)
		}
	}()
	stage := filepath.Join(tempDir, "stage")
	clone := *a.world
	if err := clone.Save(stage); err != nil {
		return "", "", "", err
	}
	if err := writePlaytestRuntime(stage, a.world); err != nil {
		return "", "", "", err
	}
	archivePath = filepath.Join(tempDir, base+"-playtest.litdworld")
	shotPath = opts.ShotPath
	if shotPath == "" {
		shotPath = filepath.Join(tempDir, "playtest.png")
	}
	host := worldpack.Hosting{
		Author:      strings.Join(a.world.Metadata.Authors, ", "),
		Title:       a.world.Metadata.Name,
		Description: a.world.Metadata.Description,
		Players: worldpack.Players{
			Min:       a.world.Metadata.Players.Min,
			Max:       a.world.Metadata.Players.Max,
			Suggested: a.world.Metadata.Players.Suggested,
		},
		Tileset:  a.world.Terrain.Tileset,
		SplatSet: a.world.Terrain.Biome,
	}
	for _, start := range a.world.Terrain.StartLocations {
		host.StartLocations = append(host.StartLocations, worldpack.StartLocation{Player: start.Player, Cell: start.Cell})
	}
	if err := worldpack.Pack(stage, archivePath, a.world.Metadata.Engine, host, nil); err != nil {
		return "", "", "", err
	}
	return tempDir, archivePath, shotPath, nil
}

// SetStartLocationsForFSV bypasses source-form validation so the command
// autotest can prove Playtest fails closed if an invalid zero-start state is
// ever present in memory. Normal editor commands must keep using
// PutStartLocationCell/AddStartLocationCell/RemoveStartLocation.
func (a *App) SetStartLocationsForFSV(starts []sourceform.StartLocation) {
	if a.world == nil {
		return
	}
	a.world.Terrain.StartLocations = append([]sourceform.StartLocation(nil), starts...)
}

func writePlaytestRuntime(stage string, w *sourceform.World) error {
	if err := writeFile(filepath.Join(stage, "data", "combat", "damage-table.toml"), []byte(playtestDamageTable)); err != nil {
		return err
	}
	if err := writeFile(filepath.Join(stage, "data", "units", "editor.toml"), []byte(playtestUnitsTOML(w.Entities))); err != nil {
		return err
	}
	if err := writeFile(filepath.Join(stage, "data", "placement", "editor.toml"), []byte(playtestPlacementTOML(w.Entities))); err != nil {
		return err
	}
	return writeFile(filepath.Join(stage, "main.lua"), []byte("Game_SetTimeOfDay(12.0)\n"))
}

const playtestDamageTable = `attack-types = ["normal", "piercing"]
armor-types = ["heavy", "light", "unarmored"]

[coefficients]
normal = [1000, 1000, 1000]
piercing = [1000, 1000, 1000]
`

func playtestUnitsTOML(entities []sourceform.Entity) string {
	types := map[string]bool{"footman": true}
	for _, ent := range entities {
		types[ent.Type] = true
	}
	keys := make([]string, 0, len(types))
	for typ := range types {
		keys = append(keys, typ)
	}
	sort.Strings(keys)
	var b strings.Builder
	for _, typ := range keys {
		armor := "heavy"
		attack := "normal"
		if strings.Contains(strings.ToLower(typ), "archer") {
			armor = "light"
			attack = "piercing"
		}
		fmt.Fprintf(&b, "[[unit]]\nid = %s\nlife = 100\narmor-type = %s\nmove-speed = 270\nturn-rate = 0.6\ncollision-size = 16\npathing = \"ground\"\n\n", strconv.Quote(typ), strconv.Quote(armor))
		fmt.Fprintf(&b, "[[unit.attack]]\ntype = %s\nrange = 90\ndamage-base = 10\ncooldown = 1.0\ndelivery = \"instant\"\ntargets-allowed = [\"ground\"]\n\n", strconv.Quote(attack))
	}
	return b.String()
}

func playtestPlacementTOML(entities []sourceform.Entity) string {
	ordered := append([]sourceform.Entity(nil), entities...)
	sort.Slice(ordered, func(i, j int) bool {
		if ordered[i].ID != ordered[j].ID {
			return ordered[i].ID < ordered[j].ID
		}
		return ordered[i].Type < ordered[j].Type
	})
	var b strings.Builder
	for _, ent := range ordered {
		fmt.Fprintf(&b, "[[unit]]\ntype = %s\nowner = %d\nx = %.0f\ny = %.0f\nfacing = %.6f\n\n",
			strconv.Quote(ent.Type), ent.Player, float64(ent.Pos[0]), float64(ent.Pos[1]), float64(ent.Rotation)*360.0/65536.0)
	}
	return b.String()
}

func playtestCommand(template []string, archivePath, shotPath string) []string {
	if len(template) == 0 {
		if sibling := siblingLitdBinary(); sibling != "" {
			template = []string{sibling, "-archive", "{{archive}}", "-autotest", "-autotest-order", "-ticks", "80", "-shot", "{{shot}}"}
		} else {
			template = []string{"go", "run", "./cmd/litd", "-archive", "{{archive}}", "-autotest", "-autotest-order", "-ticks", "80", "-shot", "{{shot}}"}
		}
	}
	out := make([]string, len(template))
	for i, arg := range template {
		arg = strings.ReplaceAll(arg, "{{archive}}", archivePath)
		arg = strings.ReplaceAll(arg, "{{shot}}", shotPath)
		out[i] = arg
	}
	return out
}

func siblingLitdBinary() string {
	exe, err := os.Executable()
	if err != nil {
		return ""
	}
	dir := filepath.Dir(exe)
	for _, name := range []string{"litd", "litd.exe"} {
		path := filepath.Join(dir, name)
		if st, err := os.Stat(path); err == nil && !st.IsDir() && st.Mode()&0o111 != 0 {
			return path
		}
	}
	return ""
}

func runPlaytestProcess(cmdline []string, dir string, timeout, killAfter time.Duration) (stdout, stderr string, exitCode int, killed bool, err error) {
	if len(cmdline) == 0 {
		return "", "", -1, false, errors.New("editor playtest: empty command")
	}
	ctx := context.Background()
	cancel := func() {}
	if timeout > 0 {
		ctx, cancel = context.WithTimeout(ctx, timeout)
	}
	defer cancel()
	cmd := exec.CommandContext(ctx, cmdline[0], cmdline[1:]...)
	cmd.Dir = dir
	var outBuf, errBuf bytes.Buffer
	cmd.Stdout = &outBuf
	cmd.Stderr = &errBuf
	if err := cmd.Start(); err != nil {
		return "", "", -1, false, err
	}
	var killTimer *time.Timer
	killedByTimer := make(chan struct{}, 1)
	if killAfter > 0 {
		killTimer = time.AfterFunc(killAfter, func() {
			select {
			case killedByTimer <- struct{}{}:
			default:
			}
			_ = cmd.Process.Kill()
		})
	}
	waitErr := cmd.Wait()
	if killTimer != nil {
		killTimer.Stop()
	}
	select {
	case <-killedByTimer:
		killed = true
	default:
	}
	stdout, stderr = outBuf.String(), errBuf.String()
	exitCode = 0
	if cmd.ProcessState != nil {
		exitCode = cmd.ProcessState.ExitCode()
		if status, ok := cmd.ProcessState.Sys().(syscall.WaitStatus); ok && status.Signaled() {
			killed = true
		}
	}
	if ctx.Err() != nil {
		killed = true
		return stdout, stderr, exitCode, killed, ctx.Err()
	}
	if waitErr != nil {
		return stdout, stderr, exitCode, killed, waitErr
	}
	return stdout, stderr, exitCode, killed, nil
}

func writeFile(path string, body []byte) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	return os.WriteFile(path, body, 0o644)
}

func listDirNames(dir string) []string {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	out := make([]string, 0, len(entries))
	for _, ent := range entries {
		out = append(out, ent.Name())
	}
	sort.Strings(out)
	return out
}

func (p PlaytestSnapshot) JSON() string {
	body, _ := json.Marshal(p)
	return string(body)
}
