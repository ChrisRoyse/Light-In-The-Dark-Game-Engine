package main

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
)

// allSegments / allCombos define the #233 acceptance matrix: three segments times
// the four {material} x {projection} combinations = 12 GL runs, plus one headless
// -nogl parity run per segment.
var allSegments = []string{"typical", "max-battle", "stress"}

type combo struct{ material, projection string }

var allCombos = []combo{
	{matUnlit, projPersp},
	{matUnlit, projOrtho},
	{matPBR, projPersp},
	{matPBR, projOrtho},
}

// allRun is one row of the combined report.
type allRun struct {
	Segment            string        `json:"segment"`
	Material           string        `json:"material"`
	Projection         string        `json:"projection"`
	GL                 bool          `json:"gl"`
	ExpectedWorldDraws int           `json:"expectedWorldDraws"`
	StreamHash         string        `json:"streamHash"`
	Summary            streamSummary `json:"summary"`
	OK                 bool          `json:"ok"`
	Shot               string        `json:"shot,omitempty"`
	Dump               string        `json:"dump"`
	Err                string        `json:"err,omitempty"`
}

// allReport is the aggregate the operator/CI reads.
type allReport struct {
	Runs            []allRun `json:"runs"`
	NoGLHashParity  bool     `json:"noglHashParity"`
	AllOK           bool     `json:"allOk"`
	HashParityNotes []string `json:"hashParityNotes,omitempty"`
}

// runAll orchestrates the full matrix by re-execing this binary once per run (the
// GL context is a per-process singleton, so each combo must be its own process).
// It collects each child's dump and writes a combined report + per-combo keyframes.
func runAll(outDir string) error {
	if outDir == "" {
		outDir = "artifacts"
	}
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return err
	}
	self, err := os.Executable()
	if err != nil {
		return err
	}
	report := allReport{AllOK: true, NoGLHashParity: true}

	for _, seg := range allSegments {
		// GL runs: all four combos.
		var glHash string
		for _, c := range allCombos {
			base := fmt.Sprintf("bench-%s-%s-%s", seg, c.material, c.projection)
			shot := filepath.Join(outDir, base+".png")
			dump := filepath.Join(outDir, base+".json")
			run := allRun{Segment: seg, Material: c.material, Projection: c.projection, GL: true, Shot: shot, Dump: dump}
			d, err := execChild(self, []string{
				"-scene", seg, "-material", c.material, "-projection", c.projection,
				"-shot", shot, "-dump", dump,
			}, dump)
			if err != nil {
				run.OK = false
				run.Err = err.Error()
				report.AllOK = false
			} else {
				run.ExpectedWorldDraws = d.ExpectedWorldDraws
				run.StreamHash = d.StreamHash
				run.Summary = d.Summary
				run.OK = d.OK
				if !d.OK {
					report.AllOK = false
				}
				if glHash == "" {
					glHash = d.StreamHash
				}
			}
			report.Runs = append(report.Runs, run)
		}

		// Headless -nogl parity run: must reproduce the GL stream hash (same
		// material/projection axis is irrelevant to the hash inputs other than the
		// material; use the canonical unlit/persp combo to compare against glHash,
		// which was the first combo = unlit/persp).
		dump := filepath.Join(outDir, fmt.Sprintf("bench-%s-nogl.json", seg))
		run := allRun{Segment: seg, Material: matUnlit, Projection: projPersp, GL: false, Dump: dump}
		d, err := execChild(self, []string{"-scene", seg, "-nogl", "-dump", dump}, dump)
		if err != nil {
			run.OK = false
			run.Err = err.Error()
			report.AllOK = false
			report.NoGLHashParity = false
		} else {
			run.ExpectedWorldDraws = d.ExpectedWorldDraws
			run.StreamHash = d.StreamHash
			run.Summary = d.Summary
			run.OK = true
			if glHash != "" && d.StreamHash != glHash {
				report.NoGLHashParity = false
				report.HashParityNotes = append(report.HashParityNotes,
					fmt.Sprintf("%s: nogl hash %s != gl(unlit/persp) hash %s", seg, d.StreamHash, glHash))
			}
		}
		report.Runs = append(report.Runs, run)
	}

	reportPath := filepath.Join(outDir, "bench-all.json")
	if err := writeJSON(reportPath, &report); err != nil {
		return err
	}
	printAllReport(report, reportPath)
	if !report.AllOK {
		return fmt.Errorf("one or more runs failed (see %s)", reportPath)
	}
	return nil
}

// execChild runs the bench binary as a subprocess and reads back the dump it wrote.
func execChild(self string, args []string, dumpPath string) (*benchDump, error) {
	cmd := exec.Command(self, args...)
	cmd.Env = os.Environ()
	cmd.Stderr = os.Stderr
	if err := cmd.Run(); err != nil {
		return nil, fmt.Errorf("child %v: %w", args, err)
	}
	b, err := os.ReadFile(dumpPath)
	if err != nil {
		return nil, fmt.Errorf("read child dump %s: %w", dumpPath, err)
	}
	var d benchDump
	if err := json.Unmarshal(b, &d); err != nil {
		return nil, fmt.Errorf("parse child dump %s: %w", dumpPath, err)
	}
	return &d, nil
}

func printAllReport(r allReport, path string) {
	fmt.Printf("renderbench -all: %d runs, allOk=%v noglHashParity=%v report=%s\n", len(r.Runs), r.AllOK, r.NoGLHashParity, path)
	fmt.Printf("  %-11s %-6s %-6s %-4s %6s %8s %10s %9s %5s\n", "segment", "mat", "proj", "gl", "draws", "maxDraw", "avgMs", "p99Ms", "ok")
	for _, run := range r.Runs {
		maxDraw := "n/a"
		if run.Summary.MaxOpaqueDraws != nil {
			maxDraw = fmt.Sprintf("%d", *run.Summary.MaxOpaqueDraws)
		}
		fmt.Printf("  %-11s %-6s %-6s %-4v %6d %8s %10.2f %9.2f %5v\n",
			run.Segment, run.Material, run.Projection, run.GL, run.ExpectedWorldDraws,
			maxDraw, run.Summary.AvgFrameMS, run.Summary.P99FrameMS, run.OK)
	}
	for _, n := range r.HashParityNotes {
		fmt.Printf("  PARITY MISMATCH: %s\n", n)
	}
}
