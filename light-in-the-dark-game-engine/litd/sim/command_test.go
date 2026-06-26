package sim

import (
	"bytes"
	"encoding/hex"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// hashOrders is the FSV probe: a hash over every order-store column,
// the state a command application mutates.
func hashOrders(w *World) uint64 {
	h := statehash.New()
	h.WriteU32(uint32(w.Orders.Count()))
	for r := int32(0); r < w.Orders.Count(); r++ {
		h.WriteU8(w.Orders.Kind[r])
		h.WriteU32(uint32(w.Orders.Target[r]))
		h.WriteI64(int64(w.Orders.Point[r].X))
		h.WriteI64(int64(w.Orders.Point[r].Y))
		h.WriteU32(uint32(w.Orders.Entity[r]))
	}
	return h.Sum64()
}

// ownedUnit creates a unit with Owner + Order rows for player p.
func ownedUnit(t *testing.T, w *World, p uint8) EntityID {
	t.Helper()
	id, ok := w.CreateUnit(fixed.Vec2{}, 0)
	if !ok || !w.Owners.Add(w.Ents, id, p, p, p) || !w.Orders.Add(w.Ents, id) {
		t.Fatalf("unit setup failed")
	}
	return id
}

// Encode hex dump checked byte-for-byte against the input.md §8
// field table, hand-computed. X+X=Y discipline: known input, known
// expected bytes, compare the actual bytes.
func TestCommandEncode(t *testing.T) {
	r := CommandRecord{
		Version:   CommandVersion, // 1
		Tick:      0x11223344,
		Player:    7,
		Seq:       0x0102,
		Opcode:    OpMove,
		Flags:     CmdFlagQueued,
		UnitCount: 2,
		Point: fixed.Vec2{
			X: 3*fixed.One + fixed.One/2,    //  3.5  → 0x0000000380000000
			Y: -(2*fixed.One + fixed.One/4), // -2.25 → 0xFFFFFFFDC0000000
		},
	}
	r.Units[0] = 0x01000005
	r.Units[1] = 0x02000009
	enc, ok := AppendEncode(nil, &r)
	if !ok {
		t.Fatal("encode refused a valid record")
	}
	want := []byte{
		0x01,                   // version u8
		0x44, 0x33, 0x22, 0x11, // tick u32 LE
		0x07,       // playerID u8
		0x02, 0x01, // seq u16 LE
		0x00,                   // opcode u8 (Move)
		0x01,                   // flags u8 (bit 0 queued)
		0x02,                   // unit count u8
		0x05, 0x00, 0x00, 0x01, // unit[0] u32 LE (gen 1, idx 5)
		0x09, 0x00, 0x00, 0x02, // unit[1] u32 LE (gen 2, idx 9)
		0x00, 0x00, 0x00, 0x80, 0x03, 0x00, 0x00, 0x00, // X i64 LE = 3.5 in 32.32
		0x00, 0x00, 0x00, 0xC0, 0xFD, 0xFF, 0xFF, 0xFF, // Y i64 LE = -2.25 in 32.32
	}
	t.Logf("encoded   %d bytes: %s", len(enc), hex.EncodeToString(enc))
	t.Logf("hand-calc %d bytes: %s", len(want), hex.EncodeToString(want))
	if !bytes.Equal(enc, want) {
		t.Fatalf("encoding does not match the §8 table")
	}
	if sz := r.EncodedSize(); sz != len(enc) {
		t.Fatalf("EncodedSize %d != actual %d", sz, len(enc))
	}
	var back CommandRecord
	n, ok := DecodeCommand(enc, &back)
	if !ok || n != len(enc) || back != r {
		t.Fatalf("round trip broken: n=%d ok=%v", n, ok)
	}
	t.Logf("round trip: decoded == original, %d bytes consumed", n)
}

// Edge 1: version mismatch → record refused. State before (queued)
// and after (rejected count, order store untouched) printed.
func TestCommandVersionMismatch(t *testing.T) {
	w := NewWorld(Caps{})
	id := ownedUnit(t, w, 0)
	r := CommandRecord{Version: 2, Opcode: OpMove, UnitCount: 1}
	r.Units[0] = id
	pre := hashOrders(w)
	if !w.StageCommand(r) {
		t.Fatal("stage refused")
	}
	t.Logf("before: staged=1 applied=%d rejected=%d orderHash=%016x",
		w.CmdApplied(), w.CmdRejected(), pre)
	w.IngestStagedCommands()
	w.Step()
	post := hashOrders(w)
	t.Logf("after:  applied=%d rejected=%d orderHash=%016x",
		w.CmdApplied(), w.CmdRejected(), post)
	if w.CmdRejected() != 1 || w.CmdApplied() != 0 || pre != post {
		t.Fatalf("version mismatch must be a counted no-op")
	}
	// encode side refuses the same record outright
	if _, ok := AppendEncode(nil, &r); ok {
		t.Fatal("AppendEncode must refuse a foreign version")
	}
}

// Edge 2: foreign-owned and dead entity IDs in the payload →
// deterministic no-op, order-store hash unchanged.
func TestCommandForeignAndDeadNoOp(t *testing.T) {
	w := NewWorld(Caps{})
	mine := ownedUnit(t, w, 0)
	dead, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.DestroyUnit(dead)

	foreign := CommandRecord{Version: CommandVersion, Opcode: OpMove, Player: 1, UnitCount: 1}
	foreign.Units[0] = mine // player 1 ordering player 0's unit
	deadRec := CommandRecord{Version: CommandVersion, Opcode: OpMove, Player: 0, UnitCount: 1}
	deadRec.Units[0] = dead // stale handle

	pre := hashOrders(w)
	w.StageCommand(foreign)
	w.StageCommand(deadRec)
	t.Logf("before: orderHash=%016x applied=%d rejected=%d", pre, w.CmdApplied(), w.CmdRejected())
	w.IngestStagedCommands()
	w.Step()
	post := hashOrders(w)
	t.Logf("after:  orderHash=%016x applied=%d rejected=%d", post, w.CmdApplied(), w.CmdRejected())
	if pre != post {
		t.Fatalf("no-op commands mutated order state: %016x -> %016x", pre, post)
	}
	if w.CmdRejected() != 2 || w.CmdApplied() != 0 {
		t.Fatalf("both records must reject: applied=%d rejected=%d", w.CmdApplied(), w.CmdRejected())
	}
}

// Edge 3: same tick + player staged seq 1 then seq 0 → applied in
// seq order 0, 1. Cross-player: player 1 staged first, player 0
// still applies first.
func TestCommandSeqOrder(t *testing.T) {
	w := NewWorld(Caps{})
	u0 := ownedUnit(t, w, 0)
	u1 := ownedUnit(t, w, 1)

	var applied []struct {
		player uint8
		seq    uint16
	}
	w.OnCommandRecord = func(tick uint32, r *CommandRecord, actors []EntityID) {
		applied = append(applied, struct {
			player uint8
			seq    uint16
		}{r.Player, r.Seq})
	}
	mk := func(p uint8, seq uint16, unit EntityID) CommandRecord {
		r := CommandRecord{Version: CommandVersion, Opcode: OpStop, Player: p, Seq: seq, UnitCount: 1}
		r.Units[0] = unit
		return r
	}
	// staging order deliberately scrambled: (p1,s0), (p0,s1), (p0,s0)
	w.StageCommand(mk(1, 0, u1))
	w.StageCommand(mk(0, 1, u0))
	w.StageCommand(mk(0, 0, u0))
	w.IngestStagedCommands()
	w.Step()
	t.Logf("staged order: (p1,s0) (p0,s1) (p0,s0); applied order: %v", applied)
	want := []struct {
		player uint8
		seq    uint16
	}{{0, 0}, {0, 1}, {1, 0}}
	if len(applied) != 3 || applied[0] != want[0] || applied[1] != want[1] || applied[2] != want[2] {
		t.Fatalf("application order must be (player, seq) sorted: %v", applied)
	}
}

// Edge 4: truncated payloads — every strict prefix of a valid
// encoding decodes to a deterministic rejection, never a panic.
func TestCommandDecodeTruncated(t *testing.T) {
	r := CommandRecord{Version: CommandVersion, Opcode: OpCastAbility,
		Player: 1, Seq: 3, UnitCount: 2, Target: 9, Data: 0x77}
	r.Units[0], r.Units[1] = 5, 6
	enc, _ := AppendEncode(nil, &r)
	rejected := 0
	for cut := 0; cut < len(enc); cut++ {
		var out CommandRecord
		n, ok := DecodeCommand(enc[:cut], &out)
		if ok || n != 0 {
			t.Fatalf("prefix len %d decoded: n=%d ok=%v", cut, n, ok)
		}
		rejected++
	}
	t.Logf("full record %d bytes; all %d strict prefixes rejected with n=0, no panic", len(enc), rejected)
}

// Valid order application: Move record writes the order head of
// every owned actor; the foreign unit in the same list is filtered.
func TestCommandMoveWritesOrderHead(t *testing.T) {
	w := NewWorld(Caps{})
	mine := ownedUnit(t, w, 0)
	other := ownedUnit(t, w, 1)
	pt := fixed.Vec2{X: 10 * fixed.One, Y: -5 * fixed.One}
	r := CommandRecord{Version: CommandVersion, Opcode: OpMove, Player: 0, UnitCount: 2, Point: pt}
	r.Units[0], r.Units[1] = mine, other
	preMine, preOther := w.Orders.Kind[w.Orders.Row(mine)], w.Orders.Kind[w.Orders.Row(other)]
	w.StageCommand(r)
	w.IngestStagedCommands()
	w.Step()
	rm, ro := w.Orders.Row(mine), w.Orders.Row(other)
	t.Logf("before: mine kind=%d other kind=%d; after: mine kind=%d point=(%v,%v) other kind=%d (untouched)",
		preMine, preOther, w.Orders.Kind[rm], w.Orders.Point[rm].X, w.Orders.Point[rm].Y, w.Orders.Kind[ro])
	if w.Orders.Kind[rm] != OrderMove || w.Orders.Point[rm] != pt {
		t.Fatalf("owned actor's order head not written")
	}
	if w.Orders.Kind[ro] != OrderStop {
		t.Fatalf("foreign actor's order head must stay untouched")
	}
	if w.CmdApplied() != 1 {
		t.Fatalf("record with one valid actor must count applied")
	}
}

func TestCommandQueuedFlagAppendsAndUnqueuedClearsFSV(t *testing.T) {
	w := NewWorld(Caps{})
	id := ownedUnit(t, w, 0)
	pt0 := fixed.Vec2{X: 10 * fixed.One, Y: 10 * fixed.One}
	pt1 := fixed.Vec2{X: 20 * fixed.One, Y: 10 * fixed.One}
	pt2 := fixed.Vec2{X: 30 * fixed.One, Y: 10 * fixed.One}
	mk := func(seq uint16, flags uint8, pt fixed.Vec2) CommandRecord {
		r := CommandRecord{Version: CommandVersion, Opcode: OpMove, Player: 0, Seq: seq, Flags: flags, UnitCount: 1, Point: pt}
		r.Units[0] = id
		return r
	}
	queuedRec := mk(1, CmdFlagQueued, pt1)
	enc, ok := AppendEncode(nil, &queuedRec)
	if !ok {
		t.Fatal("queued record encode refused")
	}
	before := dumpQueue(w, id)
	w.StageCommand(mk(0, 0, pt0))
	w.StageCommand(queuedRec)
	w.StageCommand(mk(2, CmdFlagQueued, pt2))
	w.IngestStagedCommands()
	w.Step()
	afterQueued := dumpQueue(w, id)
	qpts := commandQueuedPoints(w, id)
	t.Logf("FSV queued flag record hex=%x header.flags=%02x", enc, enc[9])
	t.Logf("FSV queue BEFORE %s", before)
	t.Logf("FSV queue AFTER  %s", afterQueued)
	if enc[9] != CmdFlagQueued {
		t.Fatalf("encoded queued flag byte = %02x, want %02x", enc[9], CmdFlagQueued)
	}
	r := w.Orders.Row(id)
	if w.Orders.Kind[r] != OrderMove || w.Orders.Point[r] != pt0 {
		t.Fatalf("current order wrong after queued command ingest: %s", afterQueued)
	}
	if w.QueueDepth(id) != 2 || len(qpts) != 2 || qpts[0] != pt1 || qpts[1] != pt2 {
		t.Fatalf("queued entries wrong: depth=%d points=%v dump=%s", w.QueueDepth(id), qpts, afterQueued)
	}

	hold := CommandRecord{Version: CommandVersion, Opcode: OpHold, Player: 0, Seq: 3, UnitCount: 1}
	hold.Units[0] = id
	w.StageCommand(hold)
	w.IngestStagedCommands()
	w.Step()
	afterClear := dumpQueue(w, id)
	t.Logf("FSV unqueued collapse AFTER %s", afterClear)
	if w.QueueDepth(id) != 0 || w.Orders.Kind[w.Orders.Row(id)] != OrderHold {
		t.Fatalf("unqueued command must clear queue and install hold: %s", afterClear)
	}
}

func TestCommandQueuedFlagOverflowDropsFSV(t *testing.T) {
	w := NewWorld(Caps{})
	id := ownedUnit(t, w, 0)
	evs := traceOrderEvents(w, id, 33)
	first := CommandRecord{Version: CommandVersion, Opcode: OpMove, Player: 0, Seq: 0, UnitCount: 1, Point: fixed.Vec2{X: fixed.One, Y: fixed.One}}
	first.Units[0] = id
	w.StageCommand(first)
	for i := 0; i < MaxOrderQueue; i++ {
		r := CommandRecord{Version: CommandVersion, Opcode: OpMove, Player: 0, Seq: uint16(i + 1), Flags: CmdFlagQueued, UnitCount: 1,
			Point: fixed.Vec2{X: fixed.FromInt(int32(i + 2)), Y: fixed.One}}
		r.Units[0] = id
		w.StageCommand(r)
	}
	extra := CommandRecord{Version: CommandVersion, Opcode: OpAttack, Player: 0, Seq: uint16(MaxOrderQueue + 1), Flags: CmdFlagQueued, UnitCount: 1}
	extra.Units[0] = id
	w.StageCommand(extra)
	w.IngestStagedCommands()
	w.Step()
	var drop *orderEv
	for i := range *evs {
		if (*evs)[i].kind == EvOrderDropped {
			drop = &(*evs)[i]
		}
	}
	t.Logf("FSV overflow queue AFTER depth=%d events=%+v dump=%s", w.QueueDepth(id), *evs, dumpQueue(w, id))
	if w.QueueDepth(id) != MaxOrderQueue {
		t.Fatalf("queue depth after overflow = %d, want %d", w.QueueDepth(id), MaxOrderQueue)
	}
	if drop == nil || drop.arg != int64(OrderAttack) {
		t.Fatalf("queued overflow must emit dropped attack event: %+v", *evs)
	}
}

func TestCommandQueuedFlagDestroyClearsQueueFSV(t *testing.T) {
	w := NewWorld(Caps{})
	id := ownedUnit(t, w, 0)
	free0 := w.OrderPoolFree()
	for i := 0; i < 3; i++ {
		r := CommandRecord{Version: CommandVersion, Opcode: OpMove, Player: 0, Seq: uint16(i), UnitCount: 1,
			Point: fixed.Vec2{X: fixed.FromInt(int32(i + 1)), Y: fixed.One}}
		if i > 0 {
			r.Flags = CmdFlagQueued
		}
		r.Units[0] = id
		w.StageCommand(r)
	}
	w.IngestStagedCommands()
	w.Step()
	beforeDestroy := dumpQueue(w, id)
	if w.QueueDepth(id) != 2 || w.OrderPoolFree() != free0-2 {
		t.Fatalf("setup queue wrong: %s poolFree=%d", beforeDestroy, w.OrderPoolFree())
	}
	w.DestroyUnit(id)
	t.Logf("FSV destroy cleanup BEFORE %s poolFree=%d AFTER orderRow=%d poolFree=%d alive=%v",
		beforeDestroy, free0-2, w.Orders.Row(id), w.OrderPoolFree(), w.Ents.Alive(id))
	if w.Orders.Row(id) != -1 || w.QueueDepth(id) != 0 || w.OrderPoolFree() != free0 {
		t.Fatalf("destroy must clear queue and recycle pool: row=%d depth=%d free=%d wantFree=%d",
			w.Orders.Row(id), w.QueueDepth(id), w.OrderPoolFree(), free0)
	}
}

// Tick assignment + late drop: Tick 0 stamps to next tick; an
// explicit already-simulated tick is dropped and counted.
func TestCommandTickAssignment(t *testing.T) {
	w := NewWorld(Caps{})
	id := ownedUnit(t, w, 0)
	w.Step() // tick = 1
	auto := CommandRecord{Version: CommandVersion, Opcode: OpStop, UnitCount: 1}
	auto.Units[0] = id
	late := auto
	late.Tick = 1 // already simulated
	var appliedAt []uint32
	w.OnCommandRecord = func(tick uint32, r *CommandRecord, actors []EntityID) {
		appliedAt = append(appliedAt, tick)
	}
	w.StageCommand(auto)
	w.StageCommand(late)
	w.IngestStagedCommands()
	w.Step() // tick = 2
	_, _, lateDrops := w.CmdDropped()
	t.Logf("at ingest tick=1: auto-tick stamped to 2, applied at %v; late (tick 1) dropped, lateDropped=%d",
		appliedAt, lateDrops)
	if len(appliedAt) != 1 || appliedAt[0] != 2 || lateDrops != 1 {
		t.Fatalf("tick assignment wrong: applied=%v lateDrops=%d", appliedAt, lateDrops)
	}
}

func commandQueuedPoints(w *World, id EntityID) []fixed.Vec2 {
	r := w.Orders.Row(id)
	if r == -1 {
		return nil
	}
	var out []fixed.Vec2
	for e := w.Orders.QueueHead[r]; e != NoOrderEntry; e = w.orderPool[e].next {
		out = append(out, w.orderPool[e].point)
	}
	return out
}

// Phase-1 record path allocates nothing at steady state (R-GC-1).
func TestCommandPathZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{})
	id := ownedUnit(t, w, 0)
	r := CommandRecord{Version: CommandVersion, Opcode: OpMove, UnitCount: 1,
		Point: fixed.Vec2{X: fixed.One, Y: fixed.One}}
	r.Units[0] = id
	for i := 0; i < 4; i++ { // warm up
		w.StageCommand(r)
		w.IngestStagedCommands()
		w.Step()
	}
	allocs := testing.AllocsPerRun(100, func() {
		for j := 0; j < 8; j++ {
			w.StageCommand(r)
		}
		w.IngestStagedCommands()
		w.Step()
	})
	t.Logf("AllocsPerRun(stage 8 + ingest + step) = %v", allocs)
	if allocs != 0 {
		t.Fatalf("command path allocated: %v", allocs)
	}
}

// FuzzCommandDecode: arbitrary bytes never panic; every accepted
// decode re-encodes to the identical bytes it consumed (canonical
// round trip — the replay-body integrity property).
func FuzzCommandDecode(f *testing.F) {
	seed := CommandRecord{Version: CommandVersion, Opcode: OpCastAbility,
		Player: 2, Seq: 7, UnitCount: 1, Target: 3, Data: 0x1234}
	seed.Units[0] = 0x01000005
	enc, _ := AppendEncode(nil, &seed)
	f.Add(enc)
	f.Add([]byte{})
	f.Add([]byte{1, 0, 0, 0, 0, 0, 0, 0, 13, 0})
	f.Fuzz(func(t *testing.T, b []byte) {
		var r CommandRecord
		n, ok := DecodeCommand(b, &r)
		if !ok {
			if n != 0 {
				t.Fatalf("rejection must consume 0 bytes, got %d", n)
			}
			return
		}
		re, encOK := AppendEncode(nil, &r)
		if !encOK {
			t.Fatalf("accepted decode must re-encode")
		}
		if !bytes.Equal(re, b[:n]) {
			t.Fatalf("round trip not canonical:\n in %x\nout %x", b[:n], re)
		}
	})
}

func BenchmarkCommandEncode(b *testing.B) {
	r := CommandRecord{Version: CommandVersion, Opcode: OpMove, UnitCount: 12,
		Point: fixed.Vec2{X: 3 * fixed.One, Y: 4 * fixed.One}}
	for i := range r.Units[:12] {
		r.Units[i] = EntityID(i + 1)
	}
	buf := make([]byte, 0, 256)
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		buf = buf[:0]
		buf, _ = AppendEncode(buf, &r)
	}
	if len(buf) == 0 {
		b.Fatal("encode produced nothing")
	}
}

func BenchmarkCommandDecode(b *testing.B) {
	r := CommandRecord{Version: CommandVersion, Opcode: OpMove, UnitCount: 12,
		Point: fixed.Vec2{X: 3 * fixed.One, Y: 4 * fixed.One}}
	for i := range r.Units[:12] {
		r.Units[i] = EntityID(i + 1)
	}
	enc, _ := AppendEncode(nil, &r)
	var out CommandRecord
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		if _, ok := DecodeCommand(enc, &out); !ok {
			b.Fatal("decode failed")
		}
	}
}
