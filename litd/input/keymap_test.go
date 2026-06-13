package input

import (
	"bytes"
	"os"
	"strings"
	"testing"
)

func TestKeymapDefaultGridFSV(t *testing.T) {
	m, err := LoadKeymap(os.DirFS("../../data"), DefaultKeymapPath)
	if err != nil {
		t.Fatal(err)
	}
	keys := m.CommandCardHotkeys(ContextGame)
	binding, ok := m.Resolve(ContextGame, Key("Q"))
	t.Logf("FSV keymap grid profile=%s slot0=%s slot11=%s Q=%+v", m.Profile, keys[0], keys[11], binding)
	if keys[0] != "Q" || keys[11] != "V" || !ok || binding.Action != CommandCardSlotAction(0) {
		t.Fatalf("grid keymap wrong: keys=%v binding=%+v ok=%v", keys, binding, ok)
	}
	if _, ok := m.Resolve(ContextGame, Key("1", "Ctrl")); !ok {
		t.Fatalf("Ctrl+1 control-group assignment not prebound")
	}
}

func TestKeymapConflictDetectionFSV(t *testing.T) {
	_, err := ReadKeymap("conflict.toml", []byte("[game]\nattack = [\"Q\"]\nmove = [\"Q\"]\n"))
	t.Logf("FSV keymap same-context conflict err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "attack") || !strings.Contains(err.Error(), "move") || !strings.Contains(err.Error(), "Q") {
		t.Fatalf("conflict error missing action names/key: %v", err)
	}
}

func TestKeymapCrossContextAndMalformedFSV(t *testing.T) {
	okMap := []byte("[game]\nmove = [\"Q\"]\n[targeting]\nconfirm = [\"Q\"]\n")
	m, err := ReadKeymap("cross.toml", okMap)
	t.Logf("FSV keymap cross-context same key err=%v bindings=%+v", err, m)
	if err != nil {
		t.Fatalf("same key across contexts should load: %v", err)
	}
	_, err = ReadKeymap("bad.toml", []byte("[game]\nmove = [\"Q\"\n"))
	t.Logf("FSV keymap malformed err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "bad.toml") {
		t.Fatalf("malformed TOML did not report file: %v", err)
	}
}

func TestKeymapRoundTripFSV(t *testing.T) {
	src := []byte("profile = \"grid\"\n\n[game]\n\"card.slot.0\" = [\"T\"]\n\"card.slot.1\" = [\"W\"]\n[targeting]\nconfirm = [\"Q\"]\n")
	m, err := ReadKeymap("roundtrip.toml", src)
	if err != nil {
		t.Fatal(err)
	}
	var first bytes.Buffer
	if err := m.WriteTOML(&first); err != nil {
		t.Fatal(err)
	}
	back, err := ReadKeymap("roundtrip.out.toml", first.Bytes())
	if err != nil {
		t.Fatal(err)
	}
	var second bytes.Buffer
	if err := back.WriteTOML(&second); err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV keymap roundtrip bytes=%q", first.String())
	if !bytes.Equal(first.Bytes(), second.Bytes()) {
		t.Fatalf("roundtrip bytes differ:\n%s\n---\n%s", first.String(), second.String())
	}
}

func TestKeymapOverlayConflictFSV(t *testing.T) {
	base := DefaultGridKeymap()
	override, err := ReadKeymap("override.toml", []byte("[game]\n\"card.slot.0\" = [\"W\"]\n"))
	if err != nil {
		t.Fatal(err)
	}
	_, err = base.Overlay(override)
	t.Logf("FSV keymap overlay conflict err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "card.slot.0") || !strings.Contains(err.Error(), "card.slot.1") {
		t.Fatalf("overlay conflict did not name both actions: %v", err)
	}
}

func TestKeymapGridVsClassicFSV(t *testing.T) {
	grid := DefaultGridKeymap()
	classic := DefaultClassicKeymap()
	gridKeys := grid.CommandCardHotkeys(ContextGame)
	classicKeys := classic.CommandCardHotkeys(ContextGame)
	gridMove, gridOK := grid.Resolve(ContextGame, Key(gridKeys[0]))
	classicMove, classicOK := classic.Resolve(ContextGame, Key(classicKeys[0]))
	t.Logf("FSV keymap profiles grid slot0=%s action=%s classic slot0=%s action=%s", gridKeys[0], gridMove.Action, classicKeys[0], classicMove.Action)
	if gridKeys[0] == classicKeys[0] || !gridOK || !classicOK || gridMove.Action != classicMove.Action || gridMove.Action != CommandCardSlotAction(0) {
		t.Fatalf("grid/classic should use different keys for same slot action: grid=%+v classic=%+v", gridMove, classicMove)
	}
}
