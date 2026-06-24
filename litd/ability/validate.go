package ability

// Offline validation of an ability template (PRD2 06, #598/#600). A template is
// valid when its source compiles (every reference resolves, every number is in
// range / precision-safe) against a resolver built from the names the template
// itself declares — exactly the fail-closed compile the runtime runs at world
// load, minus the live registries. tools/abilitycheck wraps this; #600 uses it
// to prove the shipped templates are real, valid specs.

import (
	"fmt"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// templateResolver resolves names against a template's declared effect lists,
// the fixed mover vocabulary, and (permissively) any non-empty event/key name —
// projectile/effect-internal references are the data layer's concern, not the
// ability compiler's. Effect-list names MUST be declared in the same file.
type templateResolver struct {
	effects map[string]bool
	nextKey uint32
}

func newTemplateResolver(t *Template) *templateResolver {
	r := &templateResolver{effects: make(map[string]bool, len(t.EffectLists))}
	for _, n := range t.EffectLists {
		r.effects[n] = true
	}
	return r
}

func (r *templateResolver) EffectListByName(name string) (data.EffectList, bool) {
	if !r.effects[name] {
		return data.EffectList{}, false
	}
	// A non-empty placeholder span: the offline validator only needs the
	// reference to resolve; the real arena offsets are bound at world load.
	return data.EffectList{Off: 0, Len: 1}, true
}

func (r *templateResolver) EventKindByName(name string) (uint16, bool) {
	if name == "" {
		return 0, false
	}
	return 64, true // any declared custom-event name is accepted offline
}

func (r *templateResolver) MoverKindByName(name string) (sim.MoverKind, bool) {
	return sim.MoverKindFromName(name)
}

func (r *templateResolver) KeyID(string) uint32 {
	r.nextKey++
	return r.nextKey
}

// Compile validates and compiles a template, returning the compiled spec.
func Compile(t Template) (sim.AbilitySpec, error) {
	return sim.CompileAbilitySpec(t.Source, newTemplateResolver(&t))
}

// Validate is Compile's error-only form.
func Validate(t Template) error {
	_, err := Compile(t)
	return err
}

// CheckTemplateRefs reports references the template names that the validator
// could not have resolved by structure alone (an effect list referenced but
// not declared in the file, or an unknown mover kind). These are the author
// errors abilitycheck surfaces beyond the compiler's own checks.
func CheckTemplateRefs(t Template) []error {
	var errs []error
	declared := make(map[string]bool, len(t.EffectLists))
	for _, n := range t.EffectLists {
		declared[n] = true
	}
	for _, n := range t.RefEffects {
		if !declared[n] {
			errs = append(errs, fmt.Errorf("ability %q: effect list %q referenced but not declared in [ability.effects]", t.Source.ID, n))
		}
	}
	for _, n := range t.RefMovers {
		if _, ok := sim.MoverKindFromName(n); !ok {
			errs = append(errs, fmt.Errorf("ability %q: unknown mover kind %q", t.Source.ID, n))
		}
	}
	return errs
}
