package main

// FSV for the goSignature parser (#267). SoT = the parsed goSig structure for
// known inputs (X+X=Y table) AND the real api-manifest.json: every enriched
// goSignature must parse without error, and the placeholder must fail closed.

import (
	"encoding/json"
	"os"
	"reflect"
	"strings"
	"testing"
)

func TestParseGoSignatureTable(t *testing.T) {
	cases := []struct {
		name string
		sig  string
		want goSig
	}{
		{"empty", "()", goSig{}},
		{"no-params-int-return", "() int", goSig{Returns: []string{"int"}}},
		{"method-style", "(o Vec2) Angle", goSig{
			Params: []goParam{{Name: "o", Type: "Vec2"}}, Returns: []string{"Angle"}}},
		{"no-return", "(p Player, msg string)", goSig{
			Params: []goParam{{Name: "p", Type: "Player"}, {Name: "msg", Type: "string"}}}},
		{"pointer-and-slice-return", "(g *Game, n int, owner Player, typ UnitType, pos Vec2, facing Angle) []Unit", goSig{
			Params: []goParam{
				{Name: "g", Type: "*Game"}, {Name: "n", Type: "int"},
				{Name: "owner", Type: "Player"}, {Name: "typ", Type: "UnitType"},
				{Name: "pos", Type: "Vec2"}, {Name: "facing", Type: "Angle"},
			}, Returns: []string{"[]Unit"}}},
		{"grouped-params", "(min, max int) int", goSig{
			Params:  []goParam{{Name: "min", Type: "int"}, {Name: "max", Type: "int"}},
			Returns: []string{"int"}}},
		{"variadic", "(p Player, opts ...UseOption)", goSig{
			Params: []goParam{{Name: "p", Type: "Player"}, {Name: "opts", Type: "UseOption", Variadic: true}}}},
		{"tuple-return", "(k string) (V, bool)", goSig{
			Params: []goParam{{Name: "k", Type: "string"}}, Returns: []string{"V", "bool"}}},
		{"generic-return", "() *Table[V]", goSig{Returns: []string{"*Table[V]"}}},
		{"func-callback-param", "(cb func(a int)) bool", goSig{
			Params: []goParam{{Name: "cb", Type: "func(a int)"}}, Returns: []string{"bool"}}},
		{"qualified-return", "() time.Duration", goSig{Returns: []string{"time.Duration"}}},
		{"generic-function", "[V any]() *Table[V]", goSig{
			TypeParams: []goParam{{Name: "V", Type: "any"}}, Returns: []string{"*Table[V]"}}},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got, err := parseGoSignature(c.sig)
			if err != nil {
				t.Fatalf("parse(%q): %v", c.sig, err)
			}
			if !reflect.DeepEqual(got, c.want) {
				t.Fatalf("parse(%q)\n got = %+v\nwant = %+v", c.sig, got, c.want)
			}
			t.Logf("FSV %q -> params=%+v returns=%v", c.sig, got.Params, got.Returns)
		})
	}
}

func TestParseGoSignatureFailsClosed(t *testing.T) {
	for _, bad := range []string{"(...)", "no parens", "(unbalanced", "(a, b)"} {
		if _, err := parseGoSignature(bad); err == nil {
			t.Fatalf("parse(%q) should have failed closed, got nil error", bad)
		} else {
			t.Logf("FSV fail-closed %q -> %v", bad, err)
		}
	}
}

// TestParseEveryManifestSignature is the at-scale FSV: load the real manifest
// and parse every mapped goSignature. Enriched signatures must all parse; the
// "(...)" placeholders must all fail closed. The count of each is reported as
// evidence (no silent skips).
func TestParseEveryManifestSignature(t *testing.T) {
	blob, err := os.ReadFile("../../api-manifest.json")
	if err != nil {
		t.Skipf("manifest not readable from test cwd: %v", err)
	}
	var m Manifest
	if err := json.Unmarshal(blob, &m); err != nil {
		t.Fatalf("unmarshal manifest: %v", err)
	}
	enriched, placeholder := 0, 0
	seen := map[string]bool{}
	for _, f := range m.Functions {
		if f.GoMapping == nil || f.GoMapping.GoSignature == "" {
			continue
		}
		sig := strings.TrimSpace(f.GoMapping.GoSignature)
		if seen[f.GoMapping.Symbol] {
			continue
		}
		seen[f.GoMapping.Symbol] = true
		if sig == "(...)" {
			placeholder++
			if _, err := parseGoSignature(sig); err == nil {
				t.Fatalf("%s placeholder %q parsed without error", f.GoMapping.Symbol, sig)
			}
			continue
		}
		enriched++
		if _, err := parseGoSignature(sig); err != nil {
			t.Fatalf("%s: enriched goSignature %q failed to parse: %v", f.GoMapping.Symbol, sig, err)
		}
	}
	if enriched == 0 {
		t.Fatal("no enriched signatures parsed — manifest path or shape wrong")
	}
	t.Logf("FSV at scale: parsed %d enriched signatures, %d placeholders fail closed", enriched, placeholder)
}
