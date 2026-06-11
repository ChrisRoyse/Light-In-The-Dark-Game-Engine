# Math, Strings & Conversion — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §5.1 R-SIM-2 determinism](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~55** | trig/sqrt/pow, typecasts (`I2S`/`S2R`/…), string ops, `StringHash`, randoms, the 21 `Convert*` enum constructors, bit ops, `GetHandleId` |
| `blizzard.j` BJs | **~41** | degree-trig wrappers (`SinBJ`), min/max/abs/sign, `AngleBetweenPoints`, `DistanceBetweenPoints`, `PolarProjectionBJ`, loop-index/`DoNothing` compiler artifacts |

## Representative JASS signatures

```jass
native Sin          takes real radians returns real
native SquareRoot   takes real x returns real
native Pow          takes real x, real power returns real
native I2S          takes integer i returns string
native S2R          takes string s returns real
native SubString    takes string source, integer start, integer end returns string
native StringHash   takes string s returns integer
native GetRandomInt takes integer lowBound, integer highBound returns integer
native GetRandomReal takes real lowBound, real highBound returns real
constant native ConvertRace takes integer i returns race

function DistanceBetweenPoints takes location locA, location locB returns real
function PolarProjectionBJ takes location source, real dist, real angle returns location
function GetRandomDirectionDeg takes nothing returns real
function RMaxBJ takes real a, real b returns real
function DoNothing takes nothing returns nothing
```

## Canonical Go surface

The most heavily tombstoned category: most entries are **superseded by the Go
standard library** and recorded as such in the audit manifest.

```go
// Tombstoned → stdlib/builtins:
//   Sin/Cos/Tan/Asin/Acos/Atan/Atan2/SquareRoot/Pow → math.*   (but see hazard 1)
//   I2S/R2S/R2SW/I2SW → strconv / fmt;  S2I/S2R → strconv
//   SubString/StringLength/StringCase → s[a:b], len, strings.ToUpper
//   RMaxBJ/RMinBJ/RAbsBJ/IMaxBJ/... → max/min builtins, math.Abs
//   ModuloInteger/ModuloReal → % / math.Mod (floored-mod semantics shim if needed)
//   GetBooleanAnd/Or, IntegerTertiaryOp, DoNothing, ForLoopIndexA/B → language features
//   And/Or/Not boolexpr → && || !
//   ConvertRace/ConvertAttackType/... (21 natives) → typed Go constants; the int→enum
//   constructors vanish because enums are typed from birth

// What is actually ported — geometry value-type methods (the BJ geometry helpers
// are real capability, kept per D4 on Vec2/Angle):
func (v Vec2) DistanceTo(o Vec2) float64        // DistanceBetweenPoints
func (v Vec2) AngleTo(o Vec2) Angle             // AngleBetweenPoints
func (v Vec2) Polar(dist float64, a Angle) Vec2 // PolarProjectionBJ
type Angle struct{ /* canonical radians/fixed */ }
func Deg(d float64) Angle
func Rad(r float64) Angle                       // Deg2Rad/Rad2Deg collapse into the type

// Randomness — sim-owned seeded PRNG (R-SIM-2), NOT math/rand:
func (g *Game) RandomInt(lo, hi int) int
func (g *Game) RandomFloat(lo, hi float64) float64
func (g *Game) RandomAngle() Angle              // GetRandomDirectionDeg
func (g *Game) SetRandomSeed(seed int64)
// RandomDistribution* (weighted choice tables) kept once:
func helpers.WeightedChoice[T any](g *Game, items []T, weights []int) T

// Survivors with engine semantics:
func StringHash(s string) int32   // exact WC3 SStrHash2 only if map-data compat ever needed; else FNV — decide in M2, tombstone otherwise
func (h Handle) ID() int64        // GetHandleId — opaque stable identity
func (g *Game) Localize(key string) string // GetLocalizedString → i18n table
```

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | degree-trig and min/max BJ wrappers dropped with their natives | `SinBJ(deg)` → `math.Sin(a.Rad())` |
| **D2** | width-formatting variants collapse | `R2SW`/`I2SW` → `fmt.Sprintf` (tombstoned) |
| **D3** | Loc-based geometry → `Vec2` methods | `DistanceBetweenPoints(locA, locB)` → `a.DistanceTo(b)` |
| **D4** | weighted-random-distribution state machine (`RandomDistReset/AddItem/Choose`) kept once | `helpers.WeightedChoice` |
| **D5** | `Deg2Rad`/`Rad2Deg` pair → the `Angle` type itself | constructors + accessors |

## Subsystem dependencies

- **sim**: the PRNG is sim state (seed in the state hash); *gameplay* trig/sqrt must go through the sim math package chosen in the M1 fixed-point-vs-float spike — not raw `math.*`.
- **render**: free to use `math.*` floats — cosmetic math needs no determinism.
- **asset**: localization tables for `Localize`.

## Porting hazards

1. **`math.Sin` is not cross-platform-deterministic at the bit level.** The tombstone "use stdlib" applies to *script convenience*, but any result feeding sim state must use the deterministic sim math package (fixed-point LUT or strictly-ordered software floats — M1 decision). The API must make the deterministic path the default for gameplay code; this is the category's entire risk.
2. **Two PRNG temptations**: anyone importing `math/rand` breaks replays silently. Lint rule in CI: `math/rand` is forbidden in gameplay packages.
3. **`StringHash` compat**: WC3's hash is a specific algorithm maps relied on for hashtable keys. Since LitD tombstones hashtables, exact-compat is likely unnecessary — but decide explicitly in M2, don't drift.
4. **JASS `%` and integer-division semantics** differ from Go on negatives (`ModuloInteger` is floored). Any ported BJ logic in `helpers` must be translated semantically, not textually.
5. **`ExecuteFunc(name string)`** (call-by-string-name) is tombstoned — no reflection-based dispatch; Go closures cover the capability. Record loudly: it's the one "dynamic" JASS feature deliberately not ported (v2 Lua embedding restores it for runtime maps, §5.5).
