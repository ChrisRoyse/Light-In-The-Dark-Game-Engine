package main

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
)

// M4 budget thresholds (milestones.md M4 exit, batching-and-draw-calls.md §9-10).
const (
	gateMaxDrawCalls   = 300
	gateFPSTypical     = 60.0
	gateFPSMaxBattle   = 30.0
	gateAllocsPerFrame = 0
)

// gateVerdict is one evaluated budget line. Enforced=false means the gate is
// tracked-only in this environment (reported, never a fabricated PASS) — the FPS
// and alloc gates are hardware/runtime dependent and only enforced on the
// reference machine (-gate-fps) or once #537 lands.
type gateVerdict struct {
	Name      string `json:"name"`
	Measured  string `json:"measured"`
	Threshold string `json:"threshold"`
	Enforced  bool   `json:"enforced"`
	Pass      bool   `json:"pass"`
	Detail    string `json:"detail,omitempty"`
}

type gateReport struct {
	Verdicts     []gateVerdict `json:"verdicts"`
	EnforcedPass bool          `json:"enforcedPass"`
}

// runGate evaluates the M4 budgets over the full segment×combo matrix plus the
// far-plane invariance check, prints a PASS/FAIL/TRACKED table, writes a JSON
// report, and returns an error iff an ENFORCED gate failed.
func runGate(outDir, variant string, enforceFPS bool) error {
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

	// Collect the 12 GL combo dumps (reuses the per-combo child process path).
	dumps := map[string]*benchDump{} // key "segment/material/projection"
	for _, seg := range allSegments {
		for _, c := range allCombos {
			base := fmt.Sprintf("gate-%s-%s-%s", seg, c.material, c.projection)
			shot := filepath.Join(outDir, base+".png")
			dump := filepath.Join(outDir, base+".json")
			d, err := execChild(self, []string{
				"-scene", seg, "-variant", variant, "-material", c.material, "-projection", c.projection,
				"-shot", shot, "-dump", dump,
			}, dump)
			if err != nil {
				return fmt.Errorf("gate run %s: %w", base, err)
			}
			dumps[seg+"/"+c.material+"/"+c.projection] = d
		}
	}

	report := gateReport{EnforcedPass: true}
	add := func(v gateVerdict) {
		report.Verdicts = append(report.Verdicts, v)
		if v.Enforced && !v.Pass {
			report.EnforcedPass = false
		}
	}

	// Gate 1 — draw calls: every frame of every run must be <= 300 (per-frame
	// ceiling, not average). ENFORCED (deterministic).
	worstDraws, worstWhere := worstDrawCalls(dumps)
	add(gateVerdict{
		Name:      "draw calls <=300/frame",
		Measured:  fmt.Sprintf("%d (%s)", worstDraws, worstWhere),
		Threshold: fmt.Sprintf("<=%d", gateMaxDrawCalls),
		Enforced:  true,
		Pass:      worstDraws <= gateMaxDrawCalls,
		Detail:    "per-frame ceiling across all segments/combos",
	})

	// Gate 2 — far-plane invariance: doubling the far plane must not change the
	// visible-graphic set (camera-and-culling §5.3). ENFORCED (deterministic).
	farDetail, farPass := evalFarPlane(self, outDir)
	add(gateVerdict{
		Name:      "far-plane invariance (2x)",
		Measured:  farDetail,
		Threshold: "visible-set diff == 0",
		Enforced:  true,
		Pass:      farPass,
		Detail:    "frame-0 VisibleGraphics at 1x vs 2x far plane, per segment",
	})

	// Gate 3 — FPS floors. TRACKED unless -gate-fps (reference hardware): headless
	// software-raster frame ms is not the reference machine's FPS, so enforcing it
	// here would fabricate a hardware measurement.
	typFPS := steadyFPS(dumps["typical/unlit/persp"])
	mbFPS := steadyFPS(dumps["max-battle/unlit/persp"])
	add(gateVerdict{
		Name:      "FPS typical >=60",
		Measured:  fmt.Sprintf("%.1f fps", typFPS),
		Threshold: ">=60",
		Enforced:  enforceFPS,
		Pass:      typFPS >= gateFPSTypical,
		Detail:    fpsDetail(enforceFPS),
	})
	add(gateVerdict{
		Name:      "FPS max-battle >=30",
		Measured:  fmt.Sprintf("%.1f fps", mbFPS),
		Threshold: ">=30",
		Enforced:  enforceFPS,
		Pass:      mbFPS >= gateFPSMaxBattle,
		Detail:    fpsDetail(enforceFPS),
	})
	// The 1,000-unit stress segment is the stretch (G3.10): recorded on recommended
	// spec, NEVER a low-tier gate — reported so a regression is visible without
	// failing the 500-unit gate.
	add(gateVerdict{
		Name:      "FPS stress (stretch)",
		Measured:  fmt.Sprintf("%.1f fps", steadyFPS(dumps["stress/unlit/persp"])),
		Threshold: "recorded only",
		Enforced:  false,
		Pass:      true,
		Detail:    "G3.10 stretch — recorded on recommended spec, not a low-tier gate",
	})

	// Gate 4 — allocs/frame. TRACKED pending #537 (stock-g3n render loop allocates
	// ~437/frame; R-GC-1 currently gates LITD paths only).
	worstAllocs, allocsWhere := worstSteadyAllocs(dumps)
	add(gateVerdict{
		Name:      "allocs/frame == 0",
		Measured:  fmt.Sprintf("%d (%s)", worstAllocs, allocsWhere),
		Threshold: fmt.Sprintf("==%d", gateAllocsPerFrame),
		Enforced:  false,
		Pass:      worstAllocs == gateAllocsPerFrame,
		Detail:    "TRACKED pending #537 (stock-g3n render allocs; R-GC-1 gates LITD paths)",
	})

	reportPath := filepath.Join(outDir, "gates.json")
	if err := writeJSON(reportPath, &report); err != nil {
		return err
	}
	printGateReport(report, reportPath)
	if !report.EnforcedPass {
		return fmt.Errorf("one or more ENFORCED gates failed (see %s)", reportPath)
	}
	return nil
}

// worstDrawCalls returns the highest per-frame total draw-call count across every
// run and a label for where it occurred — the SoT for the <=300 ceiling gate.
// Deterministic order: sorted by key so the reported "where" is stable on ties.
func worstDrawCalls(dumps map[string]*benchDump) (int, string) {
	worst, where := 0, ""
	for _, key := range sortedKeys(dumps) {
		for _, fs := range dumps[key].PerFrame {
			if fs.DrawCalls != nil && *fs.DrawCalls > worst {
				worst = *fs.DrawCalls
				where = fmt.Sprintf("%s frame %d", key, fs.Frame)
			}
		}
	}
	return worst, where
}

// worstSteadyAllocs returns the highest post-compile (frame > 0) allocation count
// across every run and where — the SoT for the allocs/frame gate.
func worstSteadyAllocs(dumps map[string]*benchDump) (int64, string) {
	var worst int64
	where := ""
	for _, key := range sortedKeys(dumps) {
		for _, fs := range dumps[key].PerFrame {
			if fs.Frame == 0 {
				continue
			}
			if fs.Allocs > worst {
				worst = fs.Allocs
				where = fmt.Sprintf("%s frame %d", key, fs.Frame)
			}
		}
	}
	return worst, where
}

func sortedKeys(dumps map[string]*benchDump) []string {
	keys := make([]string, 0, len(dumps))
	for k := range dumps {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

// evalFarPlane renders frame 0 of each segment at 1x and 2x far plane and compares
// VisibleGraphics. The whole field is within the 1x far plane, so doubling it must
// leave the visible set unchanged.
func evalFarPlane(self, outDir string) (string, bool) {
	pass := true
	parts := make([]string, 0, len(allSegments))
	for _, seg := range allSegments {
		d1 := filepath.Join(outDir, "gate-far1-"+seg+".json")
		d2 := filepath.Join(outDir, "gate-far2-"+seg+".json")
		a, err1 := execChild(self, []string{"-scene", seg, "-farmult", "1", "-dump", d1}, d1)
		b, err2 := execChild(self, []string{"-scene", seg, "-farmult", "2", "-dump", d2}, d2)
		if err1 != nil || err2 != nil || len(a.PerFrame) == 0 || len(b.PerFrame) == 0 {
			pass = false
			parts = append(parts, seg+":err")
			continue
		}
		v1, v2 := derefInt(a.PerFrame[0].Visible), derefInt(b.PerFrame[0].Visible)
		if v1 != v2 {
			pass = false
		}
		parts = append(parts, fmt.Sprintf("%s:%d==%d", seg, v1, v2))
	}
	return joinComma(parts), pass
}

// steadyFPS converts the median steady-state (post-compile) frame ms to FPS.
func steadyFPS(d *benchDump) float64 {
	if d == nil || len(d.PerFrame) < 2 {
		return 0
	}
	ms := make([]float64, 0, len(d.PerFrame)-1)
	for _, fs := range d.PerFrame[1:] { // drop frame 0 (shader compile)
		ms = append(ms, fs.FrameMS)
	}
	if len(ms) == 0 {
		return 0
	}
	sort.Float64s(ms)
	med := ms[len(ms)/2]
	if med <= 0 {
		return 0
	}
	return 1000.0 / med
}

func fpsDetail(enforced bool) string {
	if enforced {
		return "ENFORCED on reference hardware (-gate-fps)"
	}
	return "TRACKED: headless software-raster frame ms != reference UHD 620 FPS; enforce with -gate-fps on the reference machine"
}

func derefInt(p *int) int {
	if p == nil {
		return -1
	}
	return *p
}

func joinComma(parts []string) string {
	out := ""
	for i, p := range parts {
		if i > 0 {
			out += ", "
		}
		out += p
	}
	return out
}

func printGateReport(r gateReport, path string) {
	fmt.Printf("renderbench -gate: enforcedPass=%v report=%s\n", r.EnforcedPass, path)
	fmt.Printf("  %-26s %-30s %-18s %-9s %s\n", "gate", "measured", "threshold", "mode", "verdict")
	for _, v := range r.Verdicts {
		mode := "ENFORCED"
		if !v.Enforced {
			mode = "tracked"
		}
		verdict := "PASS"
		if !v.Pass {
			if v.Enforced {
				verdict = "FAIL"
			} else {
				verdict = "over (tracked)"
			}
		}
		fmt.Printf("  %-26s %-30s %-18s %-9s %s\n", v.Name, v.Measured, v.Threshold, mode, verdict)
	}
}
