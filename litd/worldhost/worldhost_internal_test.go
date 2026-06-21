package worldhost

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
)

// TestUninstallableTablesGate: every content table now has an install seam —
// abilities/items (#394), heroes (#396), resource-node types (#401) — so the
// fail-closed gate refuses nothing today. (Moved from cmd/litd with the function
// it covers, #490.)
func TestUninstallableTablesGate(t *testing.T) {
	if got := uninstallableTables(&data.Tables{Abilities: []data.Ability{{ID: "x"}}, Items: []data.Item{{ID: "y"}}}); got != "" {
		t.Errorf("abilities+items must be installable now, gate said %q", got)
	}
	if got := uninstallableTables(&data.Tables{Hero: &data.HeroTables{}}); got != "" {
		t.Errorf("hero tables must be installable now (#396), gate said %q", got)
	}
	if got := uninstallableTables(&data.Tables{Nodes: []data.ResourceNodeType{{}}}); got != "" {
		t.Errorf("resource-node tables must be installable now (#401), gate said %q", got)
	}
}
