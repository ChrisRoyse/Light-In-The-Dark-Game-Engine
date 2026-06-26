package hud

import (
	"os"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
)

func TestDefaultHUDLayoutAndBudgetFSV(t *testing.T) {
	canvas, err := NewCanvas(1366, 768, 1)
	if err != nil {
		t.Fatal(err)
	}
	h := NewDefaultHUDWithStrings(canvas, loadHUDStrings(t, "en"))
	widgets := h.Widgets()
	issues := ValidateWidgets(widgets, canvas.Width, canvas.Height)
	t.Logf("FSV default HUD canvas=%+v widgets=%d expectedGUIDrawCalls=%d issues=%v", canvas, len(widgets), h.ExpectedGUIDrawCalls(), issues)
	for _, w := range widgets {
		t.Logf("FSV widget %s kind=%s anchor=%s parent=%s rect=%+v atlas=%s", w.Name, w.Kind, w.Anchor, w.Parent, w.Rect, w.AtlasRegion)
	}
	if len(widgets) != DefaultHUDWidgetCount {
		t.Fatalf("widget count got %d want %d", len(widgets), DefaultHUDWidgetCount)
	}
	if len(issues) != 0 {
		t.Fatalf("layout issues: %+v", issues)
	}
	if h.ExpectedGUIDrawCalls() > DefaultHUDDrawCallCap {
		t.Fatalf("draw budget got %d cap %d", h.ExpectedGUIDrawCalls(), DefaultHUDDrawCallCap)
	}
}

func TestDefaultHUDDirtyUpdateFSV(t *testing.T) {
	canvas, _ := NewCanvas(1920, 1080, 1)
	h := NewDefaultHUDWithStrings(canvas, loadHUDStrings(t, "en"))
	state := DefaultHUDState()

	static := h.Update(state)
	t.Logf("FSV static update BEFORE initialized AFTER stats=%+v", static)
	if static.Repaints != 0 || static.DirtyLabels != 0 {
		t.Fatalf("static HUD should not repaint: %+v", static)
	}

	state.Gold += 25
	state.Lumber += 5
	resource := h.Update(state)
	t.Logf("FSV resource churn AFTER stats=%+v resource=%q", resource, h.Resource.String())
	if resource.ResourceRepaints != 1 || resource.DirtyLabels != 1 || resource.Repaints != 1 {
		t.Fatalf("resource churn should dirty one label: %+v", resource)
	}

	state.SelectionVersion++
	selection := h.Update(state)
	t.Logf("FSV selection churn AFTER stats=%+v selection=%q", selection, h.Selection.String())
	if selection.SelectionRebuilds != 1 || selection.DirtyLabels != 1 {
		t.Fatalf("selection churn should rebuild one label: %+v", selection)
	}
}

func TestDefaultHUDResourceFlashExpiresFSV(t *testing.T) {
	canvas, _ := NewCanvas(1920, 1080, 1)
	h := NewDefaultHUDWithStrings(canvas, loadHUDStrings(t, "en"))
	state := DefaultHUDState()

	event := h.ResourceBar.InsufficientGold(12, state.Gold)
	state.Tick = 12
	flash := h.Update(state)
	t.Logf("FSV default HUD resource flash event=%+v stats=%+v resource=%q", event, flash, h.Resource.String())
	if !strings.HasPrefix(h.Resource.String(), "!G ") || flash.ResourceRepaints != 1 {
		t.Fatalf("resource flash should repaint active error state: event=%+v stats=%+v resource=%q", event, flash, h.Resource.String())
	}

	state.Tick = 42
	expired := h.Update(state)
	t.Logf("FSV default HUD resource flash expired stats=%+v resource=%q", expired, h.Resource.String())
	if strings.HasPrefix(h.Resource.String(), "!") || expired.ResourceRepaints != 1 {
		t.Fatalf("resource flash should expire through HUD tick: stats=%+v resource=%q", expired, h.Resource.String())
	}
}

func TestDefaultHUDScenariosFSV(t *testing.T) {
	canvas, _ := NewCanvas(1366, 768, 1)
	h := NewDefaultHUDWithStrings(canvas, loadHUDStrings(t, "en"))
	stats := h.RunFSVScenarios()
	t.Logf("FSV scenarios %+v", stats)
	if stats.Static100.Repaints != 0 || stats.Static100.DirtyLabels != 0 {
		t.Fatalf("static scenario should have no repaint churn: %+v", stats.Static100)
	}
	if stats.ResourceChurn.Frames != 60 || stats.ResourceChurn.ResourceRepaints != 60 {
		t.Fatalf("resource churn should repaint resource label only: %+v", stats.ResourceChurn)
	}
	if stats.SelectionChurn.Frames != 500 || stats.SelectionChurn.SelectionRebuilds != 500 {
		t.Fatalf("selection churn should rebuild selection every frame: %+v", stats.SelectionChurn)
	}
}

func TestDefaultHUDUpdateZeroAllocFSV(t *testing.T) {
	canvas, _ := NewCanvas(2560, 1080, 1)
	h := NewDefaultHUDWithStrings(canvas, loadHUDStrings(t, "en"))
	state := DefaultHUDState()
	allocs := testing.AllocsPerRun(1000, func() {
		state.Gold++
		state.SelectionVersion++
		_ = h.Update(state)
	})
	t.Logf("FSV default HUD update allocs/op=%v resource=%q selection=%q", allocs, h.Resource.String(), h.Selection.String())
	if allocs != 0 {
		t.Fatalf("default HUD update allocated: %v", allocs)
	}
}

func TestDefaultHUDLocalizedStringsZeroAllocFSV(t *testing.T) {
	canvas, _ := NewCanvas(1366, 768, 1)
	h := NewDefaultHUDWithStrings(canvas, loadHUDStrings(t, "xx"))
	state := DefaultHUDState()
	allocs := testing.AllocsPerRun(1000, func() {
		state.Gold++
		state.Lumber++
		state.SelectionVersion++
		_ = h.Update(state)
	})
	t.Logf("FSV localized HUD update allocs/op=%v resource=%q selection=%q queue=%q groups=%q", allocs, h.Resource.String(), h.Selection.String(), h.Queue.String(), h.Groups.String())
	for _, got := range []string{h.Resource.String(), h.Selection.String(), h.Queue.String(), h.Groups.String()} {
		if !strings.Contains(got, "[xx.") {
			t.Fatalf("localized HUD buffer should contain pseudo-locale key text, got %q", got)
		}
	}
	if allocs != 0 {
		t.Fatalf("localized HUD update allocated: %v", allocs)
	}
}

func loadHUDStrings(t *testing.T, tag string) HUDStrings {
	t.Helper()
	table, err := locale.Load(os.DirFS("../../../data"), tag)
	if err != nil {
		t.Fatal(err)
	}
	return HUDStringsFromLocale(table)
}
