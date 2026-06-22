package asset

import (
	"fmt"
	"math"
)

// BakedSunConfig describes the load-time vertex-color scalar used by the
// optional low-preset baked sun path.
type BakedSunConfig struct {
	Direction [3]float32 `json:"direction"`
	MinScalar float32    `json:"minScalar"`
	MaxScalar float32    `json:"maxScalar"`
}

func DefaultBakedSunConfig() BakedSunConfig {
	return BakedSunConfig{
		Direction: [3]float32{0, 1, 0},
		MinScalar: 0.55,
		MaxScalar: 1,
	}
}

func BakedSunScalar(normal [3]float32, cfg BakedSunConfig) (float32, error) {
	if err := validateBakedSunConfig(cfg); err != nil {
		return 0, err
	}
	n, err := normalize3("normal", normal)
	if err != nil {
		return 0, err
	}
	dir, err := normalize3("direction", cfg.Direction)
	if err != nil {
		return 0, err
	}
	ndot := n[0]*dir[0] + n[1]*dir[1] + n[2]*dir[2]
	if ndot < 0 {
		ndot = 0
	}
	return cfg.MinScalar + (cfg.MaxScalar-cfg.MinScalar)*ndot, nil
}

func validateBakedSunConfig(cfg BakedSunConfig) error {
	if !finite32(cfg.MinScalar) || !finite32(cfg.MaxScalar) {
		return fmt.Errorf("baked sun scalar bounds must be finite")
	}
	if cfg.MinScalar < 0 || cfg.MaxScalar > 1 || cfg.MinScalar > cfg.MaxScalar {
		return fmt.Errorf("baked sun scalar bounds invalid: min=%g max=%g", cfg.MinScalar, cfg.MaxScalar)
	}
	if _, err := normalize3("direction", cfg.Direction); err != nil {
		return err
	}
	return nil
}

func normalize3(name string, v [3]float32) ([3]float32, error) {
	for i, c := range v {
		if !finite32(c) {
			return [3]float32{}, fmt.Errorf("baked sun %s[%d] must be finite", name, i)
		}
	}
	l := math.Sqrt(float64(v[0]*v[0] + v[1]*v[1] + v[2]*v[2]))
	if l == 0 {
		return [3]float32{}, fmt.Errorf("baked sun %s must be non-zero", name)
	}
	return [3]float32{float32(float64(v[0]) / l), float32(float64(v[1]) / l), float32(float64(v[2]) / l)}, nil
}

func finite32(v float32) bool {
	return !math.IsNaN(float64(v)) && !math.IsInf(float64(v), 0)
}
