package luabind

import (
	"bytes"
	"testing"
)

// Regression for #399. Installing the obs console module (RegisterObs) puts a table
// of Go functions at the global `obs`. SaveScripts fails closed on unserializable
// Go-function globals (#435), so before obs was folded into the builtin baseline a
// quicksave taken after the console was wired died with
// "luabind: cannot serialize a Go-function value" — observed live as cmd/game
// -autotest reporting loadOK=false. RegisterObs now markBuiltinGlobal("obs"), the
// same treatment the require shim gets (#481). This proves the save SUCCEEDS with
// obs installed, while a genuine world global is still present to serialize.
func TestSaveScriptsExcludesObsGlobalFSV(t *testing.T) {
	_, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()

	runRegisteredChunk(t, L, reg, `worldData = 7`) // a genuine, serializable world global
	RegisterObs(L, &fakeObs{dump: "log", level: 2})

	var cb bytes.Buffer
	if err := SaveScripts(L, reg, &cb); err != nil {
		t.Fatalf("SaveScripts after RegisterObs: %v — obs must be excluded from saved world globals", err)
	}
	if cb.Len() == 0 {
		t.Fatal("SaveScripts wrote 0 bytes")
	}
	t.Logf("FSV #399: SaveScripts ok with obs installed (%d bytes); the Go-function obs global is not serialized", cb.Len())
}
