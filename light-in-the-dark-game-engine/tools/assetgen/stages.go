// stages.go holds the concrete pipeline stage implementations (#56): the stub
// generator that stands in for the not-yet-built image/TTS stages (#57/#58), the
// real-binary asset checker, and the interactive curator. None of these mock the
// gate — the checker execs the actual tools/assetcheck binary and the curator
// reads real operator input.
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

// StubGenerator writes a placeholder candidate into the scratch area so the
// skeleton runs end to end before the real generators land. A .png output gets a
// valid 1x1 image (so it passes the gate); any other extension gets a labelled
// placeholder blob (which, for a .glb, deliberately fails the glTF profile gate
// — exactly what an un-generated model should do).
type StubGenerator struct{}

func (StubGenerator) Generate(it SpecItem, scratchDir string) (string, error) {
	dst := filepath.Join(scratchDir, filepath.FromSlash(it.Output))
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return "", err
	}
	switch strings.ToLower(filepath.Ext(it.Output)) {
	case ".png":
		img := image.NewRGBA(image.Rect(0, 0, 1, 1))
		img.Set(0, 0, color.RGBA{0x20, 0x20, 0x20, 0xff})
		f, err := os.Create(dst)
		if err != nil {
			return "", err
		}
		if err := png.Encode(f, img); err != nil {
			f.Close()
			return "", err
		}
		if err := f.Close(); err != nil {
			return "", err
		}
	default:
		body := fmt.Sprintf("assetgen-stub placeholder\noutput=%s\ngenerator=%s\nprompt=%s\n",
			it.Output, it.Generator, it.Prompt)
		if err := os.WriteFile(dst, []byte(body), 0o644); err != nil {
			return "", err
		}
	}
	return dst, nil
}

// ExecChecker runs the real tools/assetcheck binary at Bin over an assets dir and
// returns its findings (empty == clean). Exit 0 = clean, exit 1 = findings
// (parsed from --json), any other exit = an infrastructure error.
type ExecChecker struct {
	Bin string
}

type acFinding struct {
	Path string `json:"Path"`
	Rule string `json:"Rule"`
	Msg  string `json:"Msg"`
}

func (c ExecChecker) Check(assetsDir string) ([]string, error) {
	out, err := exec.Command(c.Bin, "--json", assetsDir).Output()
	if err == nil {
		return nil, nil // exit 0 — clean
	}
	ee, ok := err.(*exec.ExitError)
	if !ok {
		return nil, fmt.Errorf("run assetcheck: %w", err)
	}
	if ee.ExitCode() != 1 {
		return nil, fmt.Errorf("assetcheck exited %d: %s", ee.ExitCode(), strings.TrimSpace(string(ee.Stderr)))
	}
	var fs []acFinding
	if jerr := json.Unmarshal(out, &fs); jerr != nil {
		return nil, fmt.Errorf("parse assetcheck --json: %w", jerr)
	}
	lines := make([]string, len(fs))
	for i, f := range fs {
		lines[i] = f.Path + ": " + f.Rule + ": " + f.Msg
	}
	return lines, nil
}

// InteractiveCurator prompts the operator on Out and reads the accept/reject
// decision + sign-off from In. A bare accept with no sign-off name, or EOF
// (non-interactive), yields DecisionNone — the no-bypass refusal.
type InteractiveCurator struct {
	r   *bufio.Reader
	out io.Writer
}

func NewInteractiveCurator(in io.Reader, out io.Writer) *InteractiveCurator {
	return &InteractiveCurator{r: bufio.NewReader(in), out: out}
}

func (c *InteractiveCurator) Review(it SpecItem, candidate string) (Decision, string) {
	fmt.Fprintf(c.out, "candidate: %s  (category=%s prompt=%q)\n", candidate, it.Category, it.Prompt)
	fmt.Fprint(c.out, "accept? [y/N]: ")
	line, err := c.r.ReadString('\n')
	if err != nil && line == "" {
		return DecisionNone, "" // EOF — non-interactive / curate skipped
	}
	switch strings.TrimSpace(strings.ToLower(line)) {
	case "y", "yes":
		fmt.Fprint(c.out, "curator sign-off name: ")
		name, _ := c.r.ReadString('\n')
		name = strings.TrimSpace(name)
		if name == "" {
			return DecisionNone, "" // accept without sign-off is not a commit path
		}
		return DecisionAccept, name
	default:
		return DecisionReject, ""
	}
}
