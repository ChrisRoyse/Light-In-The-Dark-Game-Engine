package hud

import (
	"strings"
	"testing"
)

func TestResourceBarDirtyAndFormattingFSV(t *testing.T) {
	var text TextBuffer
	bar := NewResourceBar(&text, ResourceBarStrings{Gold: "G", Lumber: "L", Food: "F", Upkeep: "U"})
	initial := ResourceBarState{Gold: 725, Lumber: 240, FoodUsed: 18, FoodCap: 30, Upkeep: 0}
	stats := bar.Update(initial)
	t.Logf("FSV resourcebar initial stats=%+v text=%q", stats, text.String())
	if !stats.Dirty || text.String() != "G 725  L 240  F 18/30  U 0" {
		t.Fatalf("initial resource text mismatch stats=%+v text=%q", stats, text.String())
	}
	stats = bar.Update(initial)
	t.Logf("FSV resourcebar steady stats=%+v text=%q", stats, text.String())
	if stats.Dirty || stats.Repaints != 0 {
		t.Fatalf("steady resource values should not repaint: %+v", stats)
	}
	afterSpend := initial
	afterSpend.Gold -= 135
	afterSpend.FoodUsed++
	stats = bar.Update(afterSpend)
	t.Logf("FSV resourcebar after spend stats=%+v text=%q", stats, text.String())
	if !stats.Dirty || text.String() != "G 590  L 240  F 19/30  U 0" {
		t.Fatalf("after-spend text mismatch stats=%+v text=%q", stats, text.String())
	}
}

func TestResourceBarEdgesFSV(t *testing.T) {
	var text TextBuffer
	bar := NewResourceBar(&text, ResourceBarStrings{Gold: "G", Lumber: "L", Food: "F", Upkeep: "U"})
	foodCap := ResourceBarState{Gold: 999, Lumber: 888, FoodUsed: 100, FoodCap: 100, Upkeep: 2}
	bar.Update(foodCap)
	t.Logf("FSV resourcebar food cap text=%q", text.String())
	if !strings.Contains(text.String(), "F 100/100") || !strings.Contains(text.String(), "U 2") {
		t.Fatalf("food cap/upkeep formatting wrong: %q", text.String())
	}

	before := text.String()
	event := bar.InsufficientGold(12, foodCap.Gold)
	stats := bar.Update(ResourceBarState{Gold: 999, Lumber: 888, FoodUsed: 100, FoodCap: 100, Upkeep: 2, Tick: 12})
	t.Logf("FSV resourcebar insufficient event=%+v stats=%+v before=%q after=%q events=%+v", event, stats, before, text.String(), bar.FeedbackEvents())
	if event.Sound != ResourceErrorSoundInsufficientGold || event.Resource != ResourceGold || !stats.FlashVisible || !strings.HasPrefix(text.String(), "!G 999") {
		t.Fatalf("insufficient gold should flash and emit error hook, event=%+v stats=%+v text=%q", event, stats, text.String())
	}

	large := ResourceBarState{Gold: 9999, Lumber: 12000, FoodUsed: 99, FoodCap: 100, Upkeep: 3, Tick: 60}
	stats = bar.Update(large)
	t.Logf("FSV resourcebar large values stats=%+v text=%q", stats, text.String())
	if stats.FlashVisible || !strings.Contains(text.String(), "G 9999") || !strings.Contains(text.String(), "L 12000") {
		t.Fatalf("large values or flash expiry wrong stats=%+v text=%q", stats, text.String())
	}
}

func TestResourceBarZeroAllocFSV(t *testing.T) {
	var text TextBuffer
	bar := NewResourceBar(&text, ResourceBarStrings{Gold: "G", Lumber: "L", Food: "F", Upkeep: "U"})
	state := ResourceBarState{Gold: 725, Lumber: 240, FoodUsed: 18, FoodCap: 30, Upkeep: 0}
	bar.Update(state)
	allocs := testing.AllocsPerRun(1000, func() {
		state.Gold++
		state.FoodUsed = 18 + state.Gold%3
		_ = bar.Update(state)
	})
	t.Logf("FSV resourcebar dirty refresh allocs/op=%v text=%q", allocs, text.String())
	if allocs != 0 {
		t.Fatalf("resourcebar update allocated: %v", allocs)
	}
}
