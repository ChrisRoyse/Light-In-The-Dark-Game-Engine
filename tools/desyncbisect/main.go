package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// exit codes: 0 = identical traces, 2 = divergence found, 1 = usage/IO error.
const (
	exitIdentical = 0
	exitError     = 1
	exitDiverged  = 2
)

func fail(format string, a ...any) {
	fmt.Fprintf(os.Stderr, "desyncbisect: "+format+"\n", a...)
	os.Exit(exitError)
}

func main() {
	aPath := flag.String("a", "", "first .litdreplay file")
	bPath := flag.String("b", "", "second .litdreplay file")
	dumpA := flag.String("dump-a", "", "optional state dump JSON for A (prints the diverging system's slice)")
	dumpB := flag.String("dump-b", "", "optional state dump JSON for B")
	flag.Parse()

	if *aPath == "" || *bPath == "" {
		fail("need -a and -b replay files")
	}
	a := loadReplay(*aPath)
	b := loadReplay(*bPath)

	div, err := Bisect(a, b)
	if err != nil {
		fail("%v", err)
	}
	if !div.Found {
		fmt.Printf("no divergence: %d/%d checkpoints, all common ticks match\n", len(a.Checkpoints), len(b.Checkpoints))
		os.Exit(exitIdentical)
	}

	fmt.Printf("DESYNC at tick %d in system %q\n", div.Tick, div.System)
	if div.Detail != "" {
		fmt.Printf("  detail: %s\n", div.Detail)
	} else {
		fmt.Printf("  A sub=%016x top=%016x\n", div.SubA, div.TopA)
		fmt.Printf("  B sub=%016x top=%016x\n", div.SubB, div.TopB)
	}
	printDumpSlice(div.System, *dumpA, *dumpB)
	os.Exit(exitDiverged)
}

func loadReplay(path string) *sim.Replay {
	f, err := os.Open(path)
	if err != nil {
		fail("open %s: %v", path, err)
	}
	defer f.Close()
	r, err := sim.DecodeReplay(f)
	if err != nil {
		fail("%v", err)
	}
	return r
}

// printDumpSlice prints the diverging system's sub-hash from each state dump,
// the relevant slice the dumps expose, so the operator can confirm the dumps
// diverge in the same system and then drill into entities/buffs by hand.
func printDumpSlice(system, dumpA, dumpB string) {
	if dumpA == "" && dumpB == "" {
		return
	}
	sa := loadDumpSub(dumpA, system)
	sb := loadDumpSub(dumpB, system)
	fmt.Printf("  state dump slice for %q:\n", system)
	fmt.Printf("    A: %s\n", sa)
	fmt.Printf("    B: %s\n", sb)
}

func loadDumpSub(path, system string) string {
	if path == "" {
		return "(no dump)"
	}
	b, err := os.ReadFile(path)
	if err != nil {
		fail("read dump %s: %v", path, err)
	}
	var d struct {
		Tick uint32            `json:"tick"`
		Subs map[string]string `json:"subs"`
	}
	if err := json.Unmarshal(b, &d); err != nil {
		fail("parse dump %s: %v", path, err)
	}
	v, ok := d.Subs[system]
	if !ok {
		return fmt.Sprintf("(tick %d: no sub for %q)", d.Tick, system)
	}
	return fmt.Sprintf("tick %d subs[%q]=%s", d.Tick, system, v)
}
