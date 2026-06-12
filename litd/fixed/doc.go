// Package fixed is the deterministic 32.32 fixed-point arithmetic core
// (determinism.md §2.4, D-2026-06-11-1). All simulation math goes through
// this package; raw arithmetic on the underlying int64 outside it is
// lint-banned (determlint, M1).
//
// # Representation
//
// F64 is a signed 32.32: value = raw / 2^32. Range ±2^31 (≈ ±2.1e9 world
// units), resolution 2^-32.
//
// # Exact semantics (frozen)
//
//   - Add/Sub/Neg: two's-complement int64 semantics — overflow WRAPS.
//     Simulation code owns its ranges; wrap is deterministic and free.
//   - Abs: Abs(MinF64) wraps to MinF64 itself (two's complement), like
//     standard integer abs.
//   - Mul: exact 128-bit magnitude product via bits.Mul64, >>32, truncated
//     toward zero, sign reapplied. If the true result magnitude exceeds
//     63 bits the low 64 bits are kept (wrap). Rounding: toward zero.
//   - Div: magnitude widened <<32 into 128 bits, bits.Div64, truncated
//     toward zero, sign reapplied. PANICS on division by zero and on
//     quotient overflow (|quotient| ≥ 2^63 after scaling), matching Go's
//     native integer-division fail-fast behavior. Gameplay code guards.
//   - Floor(): arithmetic shift >>32 — floors toward negative infinity
//     (so Floor(-0.5) == -1), exactly like float math.Floor.
//
// # Freeze note
//
// Every gameplay system depends on these exact bit patterns. After M3,
// any behavior change here is a save/replay-format break (determinism.md
// §2.4). Do not "fix" rounding, wrap, or panic behavior without a
// format-version bump and a migration decision record.
package fixed
