package render

import (
	"fmt"
	"math"

	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/light"
	"github.com/g3n/engine/math32"
)

// LightingConfig is the render-facing persistent lighting contract. It is
// intentionally independent from G3N nodes so callers can validate, dump, and
// hash authoring state before allocating scene graph objects.
type LightingConfig struct {
	AmbientColor     [3]float32 `json:"ambientColor"`
	AmbientIntensity float32    `json:"ambientIntensity"`
	SunColor         [3]float32 `json:"sunColor"`
	SunIntensity     float32    `json:"sunIntensity"`
	SunAzimuth       float32    `json:"sunAzimuth"`
	SunElevation     float32    `json:"sunElevation"`
}

type SunAmbientLights struct {
	Config  LightingConfig
	Sun     *light.Directional
	Ambient *light.Ambient
}

type SceneLightSnapshot struct {
	Kind      string     `json:"kind"`
	Color     [3]float32 `json:"color"`
	Intensity float32    `json:"intensity"`
	Position  [3]float32 `json:"position,omitempty"`
}

type SceneLightingSnapshot struct {
	Config LightingConfig       `json:"config"`
	Lights []SceneLightSnapshot `json:"lights"`
	OK     bool                 `json:"ok"`
	Errors []string             `json:"errors,omitempty"`
}

func DefaultLightingConfig() LightingConfig {
	return LightingConfigFromMapData(litmapdata.DefaultLighting())
}

func LightingConfigFromMap(m *litmapdata.Map) (LightingConfig, error) {
	if m == nil {
		return LightingConfig{}, fmt.Errorf("lighting: map is nil")
	}
	return LightingConfigFromMapData(m.Lighting), nil
}

func LightingConfigFromMapData(l litmapdata.Lighting) LightingConfig {
	return LightingConfig{
		AmbientColor:     l.AmbientColor,
		AmbientIntensity: l.AmbientIntensity,
		SunColor:         l.SunColor,
		SunIntensity:     l.SunIntensity,
		SunAzimuth:       l.SunAzimuth,
		SunElevation:     l.SunElevation,
	}
}

func (c LightingConfig) Validate() error {
	for i, v := range c.AmbientColor {
		if err := validateLightingFloat(fmt.Sprintf("ambientColor[%d]", i), v, 0, 1, true); err != nil {
			return err
		}
	}
	for i, v := range c.SunColor {
		if err := validateLightingFloat(fmt.Sprintf("sunColor[%d]", i), v, 0, 1, true); err != nil {
			return err
		}
	}
	if err := validateLightingFloat("ambientIntensity", c.AmbientIntensity, 0, litmapdata.MaxLightingIntensity, true); err != nil {
		return err
	}
	if err := validateLightingFloat("sunIntensity", c.SunIntensity, 0, litmapdata.MaxLightingIntensity, true); err != nil {
		return err
	}
	if err := validateLightingFloat("sunAzimuth", c.SunAzimuth, 0, 360, false); err != nil {
		return err
	}
	if err := validateLightingFloat("sunElevation", c.SunElevation, -90, 90, true); err != nil {
		return err
	}
	return nil
}

func validateLightingFloat(field string, v float32, lo, hi float32, hiInclusive bool) error {
	if math.IsNaN(float64(v)) || math.IsInf(float64(v), 0) {
		return fmt.Errorf("lighting: %s must be finite", field)
	}
	if v < lo || (hiInclusive && v > hi) || (!hiInclusive && v >= hi) {
		bracket := "]"
		if !hiInclusive {
			bracket = ")"
		}
		return fmt.Errorf("lighting: %s %.6g out of range [%g,%g%s", field, v, lo, hi, bracket)
	}
	return nil
}

func NewSunAmbientLights(cfg LightingConfig) (*SunAmbientLights, error) {
	if err := cfg.Validate(); err != nil {
		return nil, err
	}
	sun := light.NewDirectional(color3(cfg.SunColor), cfg.SunIntensity)
	SetDirectionalSunPosition(sun, cfg.SunAzimuth, cfg.SunElevation)
	ambient := light.NewAmbient(color3(cfg.AmbientColor), cfg.AmbientIntensity)
	return &SunAmbientLights{Config: cfg, Sun: sun, Ambient: ambient}, nil
}

func AddSunAmbient(scene *core.Node, cfg LightingConfig) (*SunAmbientLights, error) {
	if scene == nil {
		return nil, fmt.Errorf("lighting: scene is nil")
	}
	snapshot := SnapshotSceneLighting(scene, cfg)
	for _, l := range snapshot.Lights {
		switch l.Kind {
		case "Directional", "Ambient":
			return nil, fmt.Errorf("lighting: scene already contains persistent %s light", l.Kind)
		}
	}
	lights, err := NewSunAmbientLights(cfg)
	if err != nil {
		return nil, err
	}
	scene.Add(lights.Sun)
	scene.Add(lights.Ambient)
	return lights, nil
}

func SnapshotSceneLighting(root core.INode, cfg LightingConfig) SceneLightingSnapshot {
	out := SceneLightingSnapshot{Config: cfg, OK: true}
	if root == nil {
		out.OK = false
		out.Errors = append(out.Errors, "scene root is nil")
		return out
	}
	var walk func(core.INode)
	walk = func(n core.INode) {
		switch l := n.(type) {
		case *light.Directional:
			c := l.Color()
			p := l.Position()
			out.Lights = append(out.Lights, SceneLightSnapshot{
				Kind:      "Directional",
				Color:     [3]float32{c.R, c.G, c.B},
				Intensity: l.Intensity(),
				Position:  [3]float32{p.X, p.Y, p.Z},
			})
		case *light.Ambient:
			c := l.Color()
			out.Lights = append(out.Lights, SceneLightSnapshot{
				Kind:      "Ambient",
				Color:     [3]float32{c.R, c.G, c.B},
				Intensity: l.Intensity(),
			})
		case *light.Point:
			out.Lights = append(out.Lights, SceneLightSnapshot{Kind: "Point"})
		case *light.Spot:
			out.Lights = append(out.Lights, SceneLightSnapshot{Kind: "Spot"})
		}
		for _, child := range n.Children() {
			walk(child)
		}
	}
	walk(root)
	if len(out.Lights) != 2 {
		out.OK = false
		out.Errors = append(out.Errors, fmt.Sprintf("persistent light count = %d, want 2", len(out.Lights)))
		return out
	}
	if out.Lights[0].Kind != "Directional" || out.Lights[1].Kind != "Ambient" {
		out.OK = false
		out.Errors = append(out.Errors, fmt.Sprintf("persistent light order = [%s,%s], want [Directional,Ambient]", out.Lights[0].Kind, out.Lights[1].Kind))
	}
	return out
}

func SunPosition(azimuth, elevation float32) math32.Vector3 {
	az := math32.DegToRad(azimuth)
	el := math32.DegToRad(elevation)
	r := float32(100)
	cosEl := math32.Cos(el)
	return math32.Vector3{
		X: r * cosEl * math32.Sin(az),
		Y: r * math32.Sin(el),
		Z: r * cosEl * math32.Cos(az),
	}
}

func SetDirectionalSunPosition(sun *light.Directional, azimuth, elevation float32) {
	pos := SunPosition(azimuth, elevation)
	sun.SetPosition(pos.X, pos.Y, pos.Z)
}

func color3(c [3]float32) *math32.Color {
	return &math32.Color{R: c[0], G: c[1], B: c[2]}
}
