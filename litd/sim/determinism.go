// The determinism harness: the spikes/fixedpoint 10k-tick workload
// promoted onto the production packages (determinism.md §3). A World
// runs a fixed seed plus a scripted command stream through litd/fixed
// movement/distance/damage math, litd/prng draws, and litd/sim/sched
// suspensions, and emits a 64-bit litd/statehash top hash every
// HashEvery ticks — the 100-entry trace that localizes any divergence
// to a 100-tick window. The CI matrix and golden cross-arch tests run
// this same harness.
package sim

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/prng"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/sched"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// Command is one entry of the scripted command stream (determinism.md
// §3: fixed seed + scripted commands define a run completely).
type Command struct {
	Tick uint32
	Kind uint8 // 0 = retarget, 1 = impulse, 2 = damage
	A, B int32
}

const (
	cmdRetarget uint8 = iota
	cmdImpulse
	cmdDamage
)

type entity struct {
	pos, vel fixed.Vec2
	hp       fixed.F64
	target   int32
}

const (
	contPulse  sched.ContID = 1 // periodic heal pulse, reschedules itself
	contWatch  sched.ContID = 2 // event waiter, re-arms on every sync
	evSync     sched.EventID = 1
	syncPeriod uint32        = 64
)

// World is the deterministic workload state.
type World struct {
	ents  []entity
	rng   *prng.Stream
	sched *sched.Scheduler
	tick  uint32

	reg     *statehash.Registry
	hUnits  *statehash.Hasher
	hPRNG   *statehash.Hasher
	hSched  *statehash.Hasher
	snap    statehash.Snapshot
	cmds    []Command
	nextCmd int
}

// NewWorld builds the workload: n entities placed from the seed's
// master stream, 8 pulse scripts on the scheduler, 4 event watchers.
func NewWorld(seed uint64, n int, cmds []Command) *World {
	w := &World{
		ents:  make([]entity, n),
		rng:   prng.New(seed, 0),
		sched: sched.New(),
		reg:   statehash.NewRegistry(),
		cmds:  cmds,
	}
	w.hUnits = w.reg.Register("units")
	w.hPRNG = w.reg.Register("prng")
	w.hSched = w.reg.Register("sched")

	for i := range w.ents {
		e := &w.ents[i]
		e.pos = fixed.Vec2{
			X: fixed.FromInt(int32(w.rng.Uint32()%8192)) / 64,
			Y: fixed.FromInt(int32(w.rng.Uint32()%8192)) / 64,
		}
		e.hp = fixed.FromInt(100)
		e.target = int32(w.rng.Uint32() % uint32(n))
	}

	w.sched.Register(contPulse, func(s *sched.Scheduler, st sched.State) {
		draw := w.rng.Uint32()
		i := int(draw) % len(w.ents)
		if i < 0 {
			i = -i
		}
		w.ents[i].hp += fixed.FromInt(1)
		s.After(uint32(draw%7)+1, contPulse, st)
	})
	w.sched.Register(contWatch, func(s *sched.Scheduler, st sched.State) {
		i := int(st[0]) % len(w.ents)
		w.ents[i].vel = w.ents[i].vel.Scale(fixed.One / 2) // friction pulse
		s.WaitEvent(evSync, contWatch, st)
	})
	for i := 0; i < 8; i++ {
		w.sched.After(uint32(i%5)+1, contPulse, sched.State{int64(i)})
	}
	for i := 0; i < 4; i++ {
		w.sched.WaitEvent(evSync, contWatch, sched.State{int64(i * 7)})
	}
	return w
}

// Step advances exactly one tick: commands, scripts, movement/combat.
func (w *World) Step() {
	w.tick++

	for w.nextCmd < len(w.cmds) && w.cmds[w.nextCmd].Tick <= w.tick {
		c := w.cmds[w.nextCmd]
		w.nextCmd++
		n := int32(len(w.ents))
		switch c.Kind {
		case cmdRetarget:
			w.ents[c.A%n].target = c.B % n
		case cmdImpulse:
			e := &w.ents[c.A%n]
			e.vel = e.vel.Add(fixed.Vec2{X: fixed.FromInt(c.B%5 - 2), Y: fixed.FromInt(c.B%3 - 1)})
		case cmdDamage:
			w.ents[c.A%n].hp -= fixed.FromInt(c.B%20 + 1)
		}
	}

	w.sched.Step()
	if w.tick%syncPeriod == 0 {
		w.sched.FireEvent(evSync)
	}

	dt := fixed.One / 20 // 50 ms tick
	attackRange := fixed.FromInt(2)
	dmg := fixed.FromInt(25) / 2 // 12.5
	for i := range w.ents {
		e := &w.ents[i]
		t := &w.ents[e.target]
		if fixed.DistSqLess(e.pos, t.pos, attackRange) {
			t.hp -= dmg.Mul(dt)
		} else {
			d := t.pos.Sub(e.pos)
			lenSq := d.LenSq()
			if lenSq > 0 {
				dist := fixed.F64(uint64(fixed.SqrtU64(uint64(lenSq))) << 16)
				inv := fixed.One.Div(dist + fixed.One)
				e.vel = e.vel.Add(d.Scale(inv).Scale(dt))
			}
		}
		e.pos = e.pos.Add(e.vel.Scale(dt))
	}
}

// Hash writes the full state field-by-field into the per-system
// sub-hashes and returns the snapshot (top hash + per-system subs).
func (w *World) Hash() *statehash.Snapshot {
	w.reg.Reset()
	w.hUnits.WriteU32(uint32(len(w.ents)))
	for i := range w.ents {
		e := &w.ents[i]
		w.hUnits.WriteI64(int64(e.pos.X))
		w.hUnits.WriteI64(int64(e.pos.Y))
		w.hUnits.WriteI64(int64(e.vel.X))
		w.hUnits.WriteI64(int64(e.vel.Y))
		w.hUnits.WriteI64(int64(e.hp))
		w.hUnits.WriteU32(uint32(e.target))
	}
	cur := w.rng.Cursor()
	w.hPRNG.WriteU64(cur.State)
	w.hPRNG.WriteU64(cur.Inc)
	w.hSched.WriteBytes(w.sched.Save(nil))
	return w.reg.Sum(&w.snap)
}

// RunHashTrace runs ticks ticks and records the top hash every `every`
// ticks — the divergence-localizing trace (determinism.md §3).
func RunHashTrace(seed uint64, n int, cmds []Command, ticks, every int) []uint64 {
	w := NewWorld(seed, n, cmds)
	trace := make([]uint64, 0, ticks/every)
	for t := 1; t <= ticks; t++ {
		w.Step()
		if t%every == 0 {
			trace = append(trace, w.Hash().Top)
		}
	}
	return trace
}

// ScriptedCommands derives a deterministic command stream from the
// match seed's sub-stream 99 — fixed input, never wall-clock anything.
func ScriptedCommands(seed uint64, count int) []Command {
	s := prng.Split(seed, 99)
	cmds := make([]Command, count)
	tick := uint32(0)
	for i := range cmds {
		tick += s.Uint32()%37 + 13
		cmds[i] = Command{
			Tick: tick,
			Kind: uint8(s.Uint32() % 3),
			A:    int32(s.Uint32() % 4096),
			B:    int32(s.Uint32() % 4096),
		}
	}
	return cmds
}

// FirstDivergentEntry returns the index of the first differing trace
// entry, or -1 if the traces are identical (length included).
func FirstDivergentEntry(a, b []uint64) int {
	n := len(a)
	if len(b) < n {
		n = len(b)
	}
	for i := 0; i < n; i++ {
		if a[i] != b[i] {
			return i
		}
	}
	if len(a) != len(b) {
		return n
	}
	return -1
}
