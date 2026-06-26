// Package sim is the deterministic simulation core (PRD §4.1).
//
// Architecture invariant (CI-enforced by tools/importcheck): this package
// and everything under it never imports litd/render, the G3N engine, or
// any GL/window package — render reads sim state, never the reverse.
package sim
