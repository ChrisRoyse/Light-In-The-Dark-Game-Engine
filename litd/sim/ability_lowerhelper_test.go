package sim

// Test helpers (#628): the sim ability API now consumes the fixed-point
// data.AbilitySpecLowered; authoring tests still build the float-bearing
// data.AbilitySpecSource, so these lower-then-call wrappers keep the tests
// terse. Numeric validation (negative mana, precision loss, out-of-range) now
// surfaces from LowerAbilitySpec; name/op resolution from Compile — both are
// folded into one error here, matching the old single-call behavior. Test files
// are determlint-exempt, so floats are fine here.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
)

func compileSrc(src data.AbilitySpecSource, res AbilityResolver) (AbilitySpec, error) {
	lo, err := data.LowerAbilitySpec(src)
	if err != nil {
		return AbilitySpec{}, err
	}
	return CompileAbilitySpec(lo, res)
}

func (w *World) registerSrcAuto(src data.AbilitySpecSource) (uint16, error) {
	lo, err := data.LowerAbilitySpec(src)
	if err != nil {
		return 0, err
	}
	return w.RegisterAbilitySpecAuto(lo)
}

func (w *World) registerSrc(src data.AbilitySpecSource, res AbilityResolver) (uint16, error) {
	lo, err := data.LowerAbilitySpec(src)
	if err != nil {
		return 0, err
	}
	return w.RegisterAbilitySpec(lo, res)
}
