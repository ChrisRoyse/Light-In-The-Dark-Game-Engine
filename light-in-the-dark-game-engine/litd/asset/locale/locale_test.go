package locale

import (
	"os"
	"strings"
	"testing"
	"testing/fstest"
)

func TestLocaleLoadRealTablesFSV(t *testing.T) {
	for _, tag := range []string{"en", "xx"} {
		t.Run(tag, func(t *testing.T) {
			table, err := Load(os.DirFS("../../../data"), tag)
			if err != nil {
				t.Fatal(err)
			}
			t.Logf("FSV locale %s keyCount=%d resourceGold=%q menuOK=%q", tag, len(table.Strings), table.Must(HUDResourceGold), table.Must(HUDMenuOKTrue))
			if got, want := len(table.Strings), len(RequiredKeys()); got != want {
				t.Fatalf("key count=%d want %d", got, want)
			}
		})
	}
}

func TestLocaleMissingAndUnusedFSV(t *testing.T) {
	blob, err := os.ReadFile("../../../data/locale/en.toml")
	if err != nil {
		t.Fatal(err)
	}
	good := string(blob)
	missing := strings.Replace(good, `"hud.queue.prefix" = "queue v"`+"\n", "", 1)
	_, err = Load(fstest.MapFS{"locale/en.toml": &fstest.MapFile{Data: []byte(missing)}}, "en")
	t.Logf("FSV missing-key error=%v", err)
	if err == nil || !strings.Contains(err.Error(), "hud.queue.prefix") {
		t.Fatalf("missing key should name hud.queue.prefix, got %v", err)
	}

	unused := good + `"hud.extra.unused" = "unused"` + "\n"
	_, err = Load(fstest.MapFS{"locale/en.toml": &fstest.MapFile{Data: []byte(unused)}}, "en")
	t.Logf("FSV unused-key error=%v", err)
	if err == nil || !strings.Contains(err.Error(), "hud.extra.unused") {
		t.Fatalf("unused key should be rejected, got %v", err)
	}
}

func TestLocaleInvalidTagAndUnknownFieldFSV(t *testing.T) {
	_, err := Load(fstest.MapFS{}, "../en")
	t.Logf("FSV invalid tag error=%v", err)
	if err == nil || !strings.Contains(err.Error(), "invalid tag") {
		t.Fatalf("invalid tag should fail closed, got %v", err)
	}

	_, err = Load(fstest.MapFS{"locale/en.toml": &fstest.MapFile{Data: []byte("name = \"English\"\n")}}, "en")
	t.Logf("FSV unknown field error=%v", err)
	if err == nil || !strings.Contains(err.Error(), "unknown field") {
		t.Fatalf("unknown field should fail closed, got %v", err)
	}
}
