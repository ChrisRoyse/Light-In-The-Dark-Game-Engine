package render

import (
	"github.com/g3n/engine/light"
	"github.com/g3n/engine/math32"
)

// DayNightSunFloor is the minimum directional-light intensity used at
// night so the scene remains readable.
const DayNightSunFloor float32 = 0.05

type dayNightKey struct {
	hour             float32
	ambient          math32.Color
	ambientIntensity float32
	sun              math32.Color
	sunIntensity     float32
	azimuth          float32
	elevation        float32
}

var dayNightKeys = [...]dayNightKey{
	{hour: 0, ambient: math32.Color{R: 0.05, G: 0.07, B: 0.14}, ambientIntensity: 0.18, sun: math32.Color{R: 0.22, G: 0.28, B: 0.45}, sunIntensity: DayNightSunFloor, azimuth: 300, elevation: -35},
	{hour: 5, ambient: math32.Color{R: 0.08, G: 0.10, B: 0.18}, ambientIntensity: 0.22, sun: math32.Color{R: 0.45, G: 0.38, B: 0.50}, sunIntensity: DayNightSunFloor, azimuth: 70, elevation: -6},
	{hour: 6, ambient: math32.Color{R: 0.25, G: 0.20, B: 0.20}, ambientIntensity: 0.35, sun: math32.Color{R: 1.00, G: 0.45, B: 0.24}, sunIntensity: 0.35, azimuth: 85, elevation: 4},
	{hour: 7, ambient: math32.Color{R: 0.55, G: 0.52, B: 0.48}, ambientIntensity: 0.48, sun: math32.Color{R: 1.00, G: 0.70, B: 0.45}, sunIntensity: 0.75, azimuth: 100, elevation: 15},
	{hour: 12, ambient: math32.Color{R: 0.82, G: 0.88, B: 1.00}, ambientIntensity: 0.62, sun: math32.Color{R: 1.00, G: 0.96, B: 0.86}, sunIntensity: 1.05, azimuth: 180, elevation: 65},
	{hour: 17, ambient: math32.Color{R: 0.62, G: 0.57, B: 0.50}, ambientIntensity: 0.50, sun: math32.Color{R: 1.00, G: 0.72, B: 0.42}, sunIntensity: 0.72, azimuth: 260, elevation: 16},
	{hour: 18, ambient: math32.Color{R: 0.32, G: 0.22, B: 0.24}, ambientIntensity: 0.36, sun: math32.Color{R: 1.00, G: 0.42, B: 0.22}, sunIntensity: 0.35, azimuth: 275, elevation: 4},
	{hour: 19, ambient: math32.Color{R: 0.09, G: 0.08, B: 0.16}, ambientIntensity: 0.24, sun: math32.Color{R: 0.42, G: 0.32, B: 0.50}, sunIntensity: DayNightSunFloor, azimuth: 290, elevation: -8},
	{hour: 24, ambient: math32.Color{R: 0.05, G: 0.07, B: 0.14}, ambientIntensity: 0.18, sun: math32.Color{R: 0.22, G: 0.28, B: 0.45}, sunIntensity: DayNightSunFloor, azimuth: 300, elevation: -35},
}

// DayNight maps a 0..24 game clock hour to ambient and sun light node
// state. It owns no scene graph nodes; callers pass the G3N lights it
// mutates each frame.
type DayNight struct {
	Ambient *light.Ambient
	Sun     *light.Directional

	ambientColor math32.Color
	sunColor     math32.Color

	// dimSub is the amount subtracted from a full (1.0) light scale by the
	// Flicker's dim phase (#500/#170), so the zero value means "undimmed" — a
	// DayNight built without NewDayNight, or never told to dim, renders at full
	// curve. The effective multiplier is 1-dimSub, applied on top of the
	// day/night curve and re-applied every Update so it persists across frames.
	// The sun keeps its DayNightSunFloor after dimming so the scene stays
	// readable even at full dim.
	dimSub float32
}

// NewDayNight creates a day/night driver for existing G3N light nodes.
func NewDayNight(ambient *light.Ambient, sun *light.Directional) *DayNight {
	return &DayNight{Ambient: ambient, Sun: sun}
}

// SetFlickerDim sets the Flicker dim multiplier (#500) applied on top of the
// day/night curve. factor clamps to [0,1]; 1 restores the undimmed curve, 0 is
// full dim (still floored at DayNightSunFloor for the sun). Takes effect on the
// next Update (callers Update every frame). This is how a running game darkens
// its ambient + sun when the beacon pulse enters its dim phase, without
// disturbing the time-of-day curve itself.
func (d *DayNight) SetFlickerDim(factor float32) {
	if factor < 0 {
		factor = 0
	} else if factor > 1 {
		factor = 1
	}
	d.dimSub = 1 - factor
}

// FlickerDim reports the current dim multiplier (1 when undimmed).
func (d *DayNight) FlickerDim() float32 { return 1 - d.dimSub }

// Update applies the day/night curve at todHours. The input wraps onto
// [0,24), so 24.0 is continuous with midnight.
func (d *DayNight) Update(todHours float64) {
	k0, k1, t := dayNightSegment(float32(todHours))
	applyDayNight(d, k0, k1, t)
}

func dayNightSegment(hour float32) (dayNightKey, dayNightKey, float32) {
	hour = normalizeDayNightHour(hour)
	for i := range dayNightKeys {
		if hour == dayNightKeys[i].hour {
			return dayNightKeys[i], dayNightKeys[i], 0
		}
	}
	for i := 0; i < len(dayNightKeys)-1; i++ {
		a, b := dayNightKeys[i], dayNightKeys[i+1]
		if hour >= a.hour && hour < b.hour {
			return a, b, (hour - a.hour) / (b.hour - a.hour)
		}
	}
	return dayNightKeys[0], dayNightKeys[0], 0
}

func normalizeDayNightHour(hour float32) float32 {
	if hour < 0 || hour >= 24 {
		hour = math32.Mod(hour, 24)
		if hour < 0 {
			hour += 24
		}
	}
	return hour
}

func applyDayNight(d *DayNight, a, b dayNightKey, t float32) {
	d.ambientColor.Set(
		lerp32(a.ambient.R, b.ambient.R, t),
		lerp32(a.ambient.G, b.ambient.G, t),
		lerp32(a.ambient.B, b.ambient.B, t),
	)
	d.sunColor.Set(
		lerp32(a.sun.R, b.sun.R, t),
		lerp32(a.sun.G, b.sun.G, t),
		lerp32(a.sun.B, b.sun.B, t),
	)
	dim := d.FlickerDim()
	ambientIntensity := lerp32(a.ambientIntensity, b.ambientIntensity, t) * dim
	// Dim the sun too, then re-floor so a dim flicker phase never blacks out the
	// scene below the night-readability minimum.
	sunIntensity := math32.Max(DayNightSunFloor, lerp32(a.sunIntensity, b.sunIntensity, t)*dim)
	azimuth := lerp32(a.azimuth, b.azimuth, t)
	elevation := lerp32(a.elevation, b.elevation, t)

	d.Ambient.SetColor(&d.ambientColor)
	d.Ambient.SetIntensity(ambientIntensity)
	d.Sun.SetColor(&d.sunColor)
	d.Sun.SetIntensity(sunIntensity)
	setSunPosition(d.Sun, azimuth, elevation)
}

func setSunPosition(sun *light.Directional, azimuth, elevation float32) {
	SetDirectionalSunPosition(sun, azimuth, elevation)
}

func lerp32(a, b, t float32) float32 {
	return a + (b-a)*t
}
