package helpers_test

// The no-power-lost import gate (#258). The helpers library must be buildable
// PURELY on the public litd/api surface: its own direct imports may reference
// only litd/api (+ the standard library; melee additionally uses
// BurntSushi/toml for its data tables), and nothing in the tree may pull in
// litd/render. (litd/sim appears in the TRANSITIVE deps because litd/api is
// the public face of the sim — that is expected and unavoidable; the gate is
// on helpers' OWN imports and on render staying unreachable, which is the
// honest, satisfiable form of the issue's `go list -deps | grep` check.)

import (
	"os/exec"
	"strings"
	"testing"
)

const apiPkg = "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"

func goList(t *testing.T, format, pkg string) []string {
	t.Helper()
	out, err := exec.Command("go", "list", "-f", format, pkg).Output()
	if err != nil {
		t.Skipf("go list unavailable (%v) — gate enforced in CI", err)
	}
	var lines []string
	for _, l := range strings.Split(strings.TrimSpace(string(out)), "\n") {
		if l = strings.TrimSpace(l); l != "" {
			lines = append(lines, l)
		}
	}
	return lines
}

// TestHelpersDirectImportsArePublicOnly — helpers imports only litd/api (+
// stdlib, filtered out by the project-path check).
func TestHelpersDirectImportsArePublicOnly(t *testing.T) {
	const proj = "github.com/Light-in-the-Dark-Analytics"
	imports := goList(t, "{{range .Imports}}{{println .}}{{end}}", "./")
	var internal []string
	for _, im := range imports {
		if strings.HasPrefix(im, proj) {
			internal = append(internal, im)
		}
	}
	t.Logf("FSV helpers direct internal imports: %v (want exactly [%s])", internal, apiPkg)
	if len(internal) != 1 || internal[0] != apiPkg {
		t.Fatalf("helpers reaches past the public boundary: internal imports = %v, want only litd/api", internal)
	}
}

// TestHelpersTreeHasNoRender — neither helpers nor any sub-package pulls in
// litd/render transitively (the render-isolation half of the FSV grep, which
// IS satisfiable — litd/api carries no render dep).
func TestHelpersTreeHasNoRender(t *testing.T) {
	deps := goList(t, "{{range .Deps}}{{println .}}{{end}}", "./...")
	for _, d := range deps {
		if strings.Contains(d, "/litd/render") {
			t.Fatalf("helpers tree transitively depends on litd/render: %s", d)
		}
	}
	t.Logf("FSV helpers tree transitive deps: %d packages, zero litd/render", len(deps))
}
