package render

import (
	"fmt"
	"testing"

	"github.com/g3n/engine/light"
	"github.com/g3n/engine/math32"
)

type lightState struct {
	ambient          math32.Color
	ambientIntensity float32
	sun              math32.Color
	sunIntensity     float32
	pos              math32.Vector3
}

func newTestDayNight() (*DayNight, *light.Ambient, *light.Directional) {
	ambient := light.NewAmbient(&math32.Color{}, 0)
	sun := light.NewDirectional(&math32.Color{}, 0)
	return NewDayNight(ambient, sun), ambient, sun
}

func readLightState(ambient *light.Ambient, sun *light.Directional) lightState {
	return lightState{
		ambient:          ambient.Color(),
		ambientIntensity: ambient.Intensity(),
		sun:              sun.Color(),
		sunIntensity:     sun.Intensity(),
		pos:              sun.Position(),
	}
}

func stateLine(hour float64, s lightState) string {
	return fmt.Sprintf("h=%04.1f amb=(%.3f %.3f %.3f)*%.3f sun=(%.3f %.3f %.3f)*%.3f pos=(%.2f %.2f %.2f)",
		hour, s.ambient.R, s.ambient.G, s.ambient.B, s.ambientIntensity,
		s.sun.R, s.sun.G, s.sun.B, s.sunIntensity, s.pos.X, s.pos.Y, s.pos.Z)
}

func brightness(c math32.Color, intensity float32) float32 {
	return (0.2126*c.R + 0.7152*c.G + 0.0722*c.B) * intensity
}

func assertColorExact(t *testing.T, got, want math32.Color) {
	t.Helper()
	if got != want {
		t.Fatalf("color got=%+v want=%+v", got, want)
	}
}

func TestDayNightKeyframesReadBack(t *testing.T) {
	dn, ambient, sun := newTestDayNight()
	initial := readLightState(ambient, sun)
	dn.Update(0)
	midnight := readLightState(ambient, sun)
	dn.Update(12)
	noon := readLightState(ambient, sun)
	t.Logf("FSV day/night initial:  %s", stateLine(-1, initial))
	t.Logf("FSV day/night midnight: %s", stateLine(0, midnight))
	t.Logf("FSV day/night noon:     %s", stateLine(12, noon))

	if initial.ambientIntensity != 0 || initial.sunIntensity != 0 {
		t.Fatalf("test fixture lights must start unlit: %+v", initial)
	}
	assertColorExact(t, midnight.ambient, dayNightKeys[0].ambient)
	assertColorExact(t, midnight.sun, dayNightKeys[0].sun)
	assertColorExact(t, noon.ambient, dayNightKeys[4].ambient)
	assertColorExact(t, noon.sun, dayNightKeys[4].sun)
	if midnight.ambientIntensity != dayNightKeys[0].ambientIntensity || noon.ambientIntensity != dayNightKeys[4].ambientIntensity {
		t.Fatalf("ambient intensity mismatch: midnight %.3f noon %.3f", midnight.ambientIntensity, noon.ambientIntensity)
	}
	if midnight.sunIntensity != DayNightSunFloor || noon.sunIntensity != dayNightKeys[4].sunIntensity {
		t.Fatalf("sun intensity mismatch: midnight %.3f noon %.3f", midnight.sunIntensity, noon.sunIntensity)
	}
	if midnight.pos.Y >= 0 || noon.pos.Y <= 0 {
		t.Fatalf("sun horizon wrong: midnight y=%.3f noon y=%.3f", midnight.pos.Y, noon.pos.Y)
	}
	if brightness(noon.ambient, noon.ambientIntensity) <= brightness(midnight.ambient, midnight.ambientIntensity) {
		t.Fatalf("noon ambient must be brighter than midnight")
	}
}

func TestDayNightKeyExactAndWrap(t *testing.T) {
	dn, ambient, sun := newTestDayNight()
	dn.Update(6)
	dawn := readLightState(ambient, sun)
	dn.Update(24)
	wrap := readLightState(ambient, sun)
	dn.Update(0)
	midnight := readLightState(ambient, sun)
	t.Logf("FSV day/night dawn exact: %s", stateLine(6, dawn))
	t.Logf("FSV day/night wrap 24:    %s", stateLine(24, wrap))
	t.Logf("FSV day/night midnight:   %s", stateLine(0, midnight))

	assertColorExact(t, dawn.ambient, dayNightKeys[2].ambient)
	assertColorExact(t, dawn.sun, dayNightKeys[2].sun)
	if dawn.ambientIntensity != dayNightKeys[2].ambientIntensity || dawn.sunIntensity != dayNightKeys[2].sunIntensity {
		t.Fatalf("dawn exact intensities wrong: amb %.3f sun %.3f", dawn.ambientIntensity, dawn.sunIntensity)
	}
	if wrap != midnight {
		t.Fatalf("Update(24) != Update(0): 24=%+v 0=%+v", wrap, midnight)
	}
}

func TestDayNightSweepBounds(t *testing.T) {
	dn, ambient, sun := newTestDayNight()
	minSun, maxSun := float32(10), float32(0)
	for i := 0; i <= 240; i++ {
		hour := float64(i) / 10
		dn.Update(hour)
		s := readLightState(ambient, sun)
		if math32.IsNaN(s.ambient.R) || math32.IsNaN(s.ambient.G) || math32.IsNaN(s.ambient.B) ||
			math32.IsNaN(s.sun.R) || math32.IsNaN(s.sun.G) || math32.IsNaN(s.sun.B) ||
			math32.IsNaN(s.ambientIntensity) || math32.IsNaN(s.sunIntensity) ||
			math32.IsNaN(s.pos.X) || math32.IsNaN(s.pos.Y) || math32.IsNaN(s.pos.Z) {
			t.Fatalf("NaN at hour %.1f: %+v", hour, s)
		}
		if s.sunIntensity < DayNightSunFloor || s.sunIntensity > dayNightKeys[4].sunIntensity {
			t.Fatalf("sun intensity %.3f out of bounds at hour %.1f", s.sunIntensity, hour)
		}
		minSun = math32.Min(minSun, s.sunIntensity)
		maxSun = math32.Max(maxSun, s.sunIntensity)
	}
	t.Logf("FSV day/night sweep 0..24 step 0.1: minSun=%.3f maxSun=%.3f", minSun, maxSun)
	if minSun != DayNightSunFloor || maxSun != dayNightKeys[4].sunIntensity {
		t.Fatalf("sweep did not hit expected min/max: %.3f %.3f", minSun, maxSun)
	}
}

func TestDayNightUpdateZeroAlloc(t *testing.T) {
	dn, _, _ := newTestDayNight()
	dn.Update(12)
	allocs := testing.AllocsPerRun(1000, func() {
		dn.Update(17.25)
	})
	t.Logf("FSV day/night Update allocs/op = %v", allocs)
	if allocs != 0 {
		t.Fatalf("Update allocated: %v", allocs)
	}
}
