// Package render draws sim state via the G3N engine (PRD §4.1).
//
// It reads litd/sim state and never mutates it. The reverse import
// (sim → render) is banned and CI-enforced by tools/importcheck.
package render
