// options.go codifies the R-API-3 functional-options convention for litd/api.
// It declares no speculative types: each canonical verb's options live next to
// the method they serve (e.g. EventOption with OnEvent in events.go). This file
// is the single normative statement of the convention; tools/apilint is its
// mechanical enforcer.
//
// THE CONVENTION (public-api-design.md §3.3 / R-API-3, naming-and-style.md N-7)
//
//  1. The dedup policy keeps only the "complex version" of each native, so a
//     canonical verb can carry many knobs. Express the long tail as variadic
//     functional options — never a positional parameter explosion (apilint
//     G2.3 caps a verb at 5 positional params, the variadic slot excepted) and
//     never parallel function variants (the JASS *…BJ / *…Loc twins collapse
//     into one verb + options, per the D2/D3 dedup rules).
//
//         func (u Unit) Damage(target Widget, amount float64, opts ...DamageOption)
//
//  2. Option constructors are verb-less, declarative modifiers that read at the
//     call site as adjectives, not commands (N-7): litd.DamageRanged(),
//     litd.DamageType(litd.DamageMagic), litd.ForPlayer(p), litd.WithColorChange().
//
//  3. Options are plain data. A constructor records its argument; it may not run
//     side effects or mutate game state when called. (The sole behavioural
//     option is the event Where filter, which carries a pure, wait-free
//     predicate — execution-model.md §4.)
//
//  4. Zero options reproduce the most common BJ default (the G-3 contract): a
//     bare u.Damage(target, 50) is the melee-attack default that the original
//     UnitDamageTarget call gave with its common argument pattern. Each option
//     type's godoc names the originating BJ default it diverges from.
//
//  5. An option type is a named type — conventionally a func over the verb's
//     private config struct (type DamageOption func(*damageConfig)) or, where
//     naming every field at the call site helps, a single exported options
//     struct passed by value. Its name ends in "Option" (or it is a func type);
//     apilint uses that shape to permit the one package-level func that may take
//     a handle parameter — an option constructor such as ForPlayer(p Player).
//
// Everything here is enforced, not merely documented: `go run ./tools/apilint
// ./litd/api` fails the build on any positional-param overflow, any free verb
// with a handle parameter, any error-returning gameplay verb, any G3N type in a
// signature, and the forbidden trigger-zoo identifiers.

package litd
