package litd_test

// #268 seam FSV: g.LoadWorld delegates to a host-installed backend and fails
// closed when none is installed. SoT = the error returned / the path the
// backend actually received — never a trusted code.

import (
	"errors"
	"strings"
	"testing"

	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

func TestLoadWorldNoBackendFailsClosedFSV(t *testing.T) {
	g, err := litd.NewGame(litd.GameOptions{MaxUnits: 4})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if g.HasWorldLoader() {
		t.Fatal("a fresh game must have no world loader installed")
	}
	err = g.LoadWorld("worlds/whatever")
	if err == nil {
		t.Fatal("LoadWorld with no backend must fail loudly, got nil (fail-open!)")
	}
	t.Logf("FSV no-backend: %v", err)
	if !strings.Contains(err.Error(), "no world loader") {
		t.Errorf("error must name the missing backend: %v", err)
	}
}

func TestLoadWorldDelegatesToBackendFSV(t *testing.T) {
	g, err := litd.NewGame(litd.GameOptions{MaxUnits: 4})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	sentinel := errors.New("backend-ran")
	var gotPath string
	var gotGame *litd.Game
	g.SetWorldLoader(func(gg *litd.Game, path string) error {
		gotGame, gotPath = gg, path
		return sentinel
	})
	if !g.HasWorldLoader() {
		t.Fatal("HasWorldLoader must be true after SetWorldLoader")
	}

	err = g.LoadWorld("worlds/dev-sandbox")
	t.Logf("FSV delegate: backend got path=%q sameGame=%v err=%v", gotPath, gotGame == g, err)
	if gotPath != "worlds/dev-sandbox" {
		t.Errorf("backend received path %q, want \"worlds/dev-sandbox\"", gotPath)
	}
	if gotGame != g {
		t.Error("backend received a different *Game than the receiver")
	}
	if !errors.Is(err, sentinel) {
		t.Errorf("LoadWorld must return the backend's error verbatim, got %v", err)
	}

	// Clearing the backend returns to fail-closed.
	g.SetWorldLoader(nil)
	if g.HasWorldLoader() {
		t.Fatal("SetWorldLoader(nil) must clear the backend")
	}
	if g.LoadWorld("x") == nil {
		t.Fatal("after clearing, LoadWorld must fail closed again")
	}
}
