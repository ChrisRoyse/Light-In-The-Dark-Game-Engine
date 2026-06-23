// Package obs is the structured logging core (R-OBS-1, D-34,
// observability-and-debugging.md §1): a preallocated fixed-entry ring
// buffer with zero allocations per Log call on tick/frame paths.
// Message text is interned at registration; formatting is deferred to
// dump time. ERROR/WARN additionally stream to a binary disk sink
// (release builds) — decoded to text by DecodeSink.
package obs

import (
	"bufio"
	"encoding/binary"
	"fmt"
	"io"
	"os"
	"strings"
	"sync"
	"sync/atomic"
)

// Level is the log severity. Lower value = more severe.
type Level uint8

const (
	Error Level = iota
	Warn
	Info
	Debug
	Trace
	numLevels
)

var levelNames = [numLevels]string{"ERROR", "WARN", "INFO", "DEBUG", "TRACE"}

func (l Level) String() string {
	if int(l) < len(levelNames) {
		return levelNames[l]
	}
	return fmt.Sprintf("LEVEL(%d)", uint8(l))
}

// Channel identifies the emitting subsystem. New subsystem ⇒ new
// channel here (code-review gate, observability-and-debugging.md §1).
type Channel uint8

const (
	ChSimTick Channel = iota
	ChSimPath
	ChSimCombat
	ChSimSched
	ChRender
	ChAsset
	ChLua
	ChAI
	ChNet
	ChAudio
	ChUI
	NumChannels
)

var channelNames = [NumChannels]string{
	"sim.tick", "sim.path", "sim.combat", "sim.sched", "render",
	"asset", "lua", "ai", "net", "audio", "ui",
}

func (c Channel) String() string {
	if int(c) < len(channelNames) {
		return channelNames[c]
	}
	return fmt.Sprintf("chan(%d)", uint8(c))
}

// MsgID indexes the interned message table.
type MsgID uint16

// Entry is one log record: 64 bytes fixed, value types only —
// obs_test.go pins the size with unsafe.Sizeof (test-only unsafe).
type Entry struct {
	Tick    uint32
	Frame   uint32
	Level   Level
	Channel Channel
	MsgID   MsgID
	_       uint32   // padding, keeps Args 8-aligned
	Args    [4]int64 // unused args are zero and not formatted
	_       [16]byte // reserved; pads the entry to 64 bytes
}

const entryBytes = 64

// RingCap is the default ring capacity (≈ last 5 minutes of engine
// chatter at typical rates).
const RingCap = 65536

// Logger is the structured log sink. Log is safe for concurrent use
// from the sim and render goroutines: slots are claimed atomically.
// Configuration (Register, SetChannelLevel, sink) happens at startup,
// before concurrent logging begins.
type Logger struct {
	ring []Entry
	mask uint64
	n    atomic.Uint64 // total entries ever written; next slot = n & mask

	msgs      []string
	chanLevel [NumChannels]Level

	sinkMu     sync.Mutex
	sink       *bufio.Writer
	sinkFile   *os.File
	sinkBuf    [entryBytes]byte
	sinkMaxLvl Level // levels ≤ this go to disk (Warn ⇒ ERROR+WARN)
}

// New returns a Logger with a preallocated capacity-entry ring.
// capacity must be a power of two (the ring index is a mask).
func New(capacity int) *Logger {
	if capacity <= 0 || capacity&(capacity-1) != 0 {
		panic("obs: ring capacity must be a power of two")
	}
	l := &Logger{
		ring: make([]Entry, capacity),
		mask: uint64(capacity - 1),
	}
	for i := range l.chanLevel {
		l.chanLevel[i] = Trace // default: everything to the ring
	}
	l.msgs = append(l.msgs, "(unregistered message)") // MsgID 0 reserved
	return l
}

// Register interns a message format and returns its MsgID. Formats
// reference args as {0}..{3}. Call at startup only.
func (l *Logger) Register(format string) MsgID {
	if len(l.msgs) > 65535 {
		panic("obs: message table full")
	}
	l.msgs = append(l.msgs, format)
	return MsgID(len(l.msgs) - 1)
}

// SetChannelLevel sets the most-verbose level ch records (debug-build
// per-channel config). Entries more verbose than lvl are dropped.
func (l *Logger) SetChannelLevel(ch Channel, lvl Level) { l.chanLevel[ch] = lvl }

// ChannelLevel reports the most-verbose level ch currently records — the read
// counterpart of SetChannelLevel (used by the obs.* debug-console knob, #399).
func (l *Logger) ChannelLevel(ch Channel) Level { return l.chanLevel[ch] }

// AttachSink opens path and streams every entry at level ≤ maxLevel
// (Warn ⇒ ERROR/WARN, the release default) as binary records.
func (l *Logger) AttachSink(path string, maxLevel Level) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	l.sinkMu.Lock()
	l.sinkFile = f
	l.sink = bufio.NewWriterSize(f, 64*1024)
	l.sinkMaxLvl = maxLevel
	l.sinkMu.Unlock()
	return nil
}

// CloseSink flushes and closes the disk sink.
func (l *Logger) CloseSink() error {
	l.sinkMu.Lock()
	defer l.sinkMu.Unlock()
	if l.sink == nil {
		return nil
	}
	if err := l.sink.Flush(); err != nil {
		return err
	}
	err := l.sinkFile.Close()
	l.sink, l.sinkFile = nil, nil
	return err
}

// Log records one entry. Zero allocations; safe from sim and render
// goroutines concurrently (the ring assumes writers do not lap a full
// ring within one entry write).
func (l *Logger) Log(tick, frame uint32, lvl Level, ch Channel, msg MsgID, a0, a1, a2, a3 int64) {
	if lvl > l.chanLevel[ch] {
		return
	}
	idx := l.n.Add(1) - 1
	e := &l.ring[idx&l.mask]
	e.Tick = tick
	e.Frame = frame
	e.Level = lvl
	e.Channel = ch
	e.MsgID = msg
	e.Args[0], e.Args[1], e.Args[2], e.Args[3] = a0, a1, a2, a3

	if lvl <= l.sinkMaxLvl && l.sink != nil {
		l.sinkMu.Lock()
		if l.sink != nil {
			b := l.sinkBuf[:]
			binary.LittleEndian.PutUint32(b[0:], tick)
			binary.LittleEndian.PutUint32(b[4:], frame)
			b[8] = byte(lvl)
			b[9] = byte(ch)
			binary.LittleEndian.PutUint16(b[10:], uint16(msg))
			binary.LittleEndian.PutUint64(b[16:], uint64(a0))
			binary.LittleEndian.PutUint64(b[24:], uint64(a1))
			binary.LittleEndian.PutUint64(b[32:], uint64(a2))
			binary.LittleEndian.PutUint64(b[40:], uint64(a3))
			_, _ = l.sink.Write(b)
		}
		l.sinkMu.Unlock()
	}
}

// Len reports how many entries the ring currently holds (≤ capacity).
func (l *Logger) Len() int {
	n := l.n.Load()
	if n > uint64(len(l.ring)) {
		return len(l.ring)
	}
	return int(n)
}

// Total reports how many entries were ever logged (including evicted).
func (l *Logger) Total() uint64 { return l.n.Load() }

// Snapshot copies the ring oldest-to-newest into dst and returns it.
// Call from a quiesced state (dump time); concurrent writers may tear
// the oldest entries.
func (l *Logger) Snapshot(dst []Entry) []Entry {
	n := l.n.Load()
	count := n
	if count > uint64(len(l.ring)) {
		count = uint64(len(l.ring))
	}
	dst = dst[:0]
	start := n - count
	for i := uint64(0); i < count; i++ {
		dst = append(dst, l.ring[(start+i)&l.mask])
	}
	return dst
}

// FormatEntry renders one entry using the interned message table —
// the deferred-formatting half of the zero-alloc contract.
func (l *Logger) FormatEntry(e *Entry) string {
	msg := l.msgs[0]
	if int(e.MsgID) < len(l.msgs) {
		msg = l.msgs[e.MsgID]
	}
	for i := 0; i < 4; i++ {
		ph := fmt.Sprintf("{%d}", i)
		if strings.Contains(msg, ph) {
			msg = strings.ReplaceAll(msg, ph, fmt.Sprintf("%d", e.Args[i]))
		}
	}
	return fmt.Sprintf("[t%08d f%08d] %-5s %-10s %s", e.Tick, e.Frame, e.Level, e.Channel, msg)
}

// Dump writes the formatted ring contents, oldest first.
func (l *Logger) Dump(w io.Writer) error {
	entries := l.Snapshot(nil)
	bw := bufio.NewWriter(w)
	fmt.Fprintf(bw, "# litd/obs ring dump: %d entries (of %d ever logged, ring cap %d)\n",
		len(entries), l.Total(), len(l.ring))
	for i := range entries {
		if _, err := fmt.Fprintln(bw, l.FormatEntry(&entries[i])); err != nil {
			return err
		}
	}
	return bw.Flush()
}

// DumpFile writes the formatted ring to path.
func (l *Logger) DumpFile(path string) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	return l.Dump(f)
}

// DecodeSink renders a binary disk-sink stream as text (dump-time
// formatting for the release ERROR/WARN file).
func (l *Logger) DecodeSink(r io.Reader, w io.Writer) error {
	bw := bufio.NewWriter(w)
	var b [entryBytes]byte
	for {
		_, err := io.ReadFull(r, b[:])
		if err == io.EOF {
			return bw.Flush()
		}
		if err != nil {
			return fmt.Errorf("obs: sink stream truncated: %w", err)
		}
		e := Entry{
			Tick:    binary.LittleEndian.Uint32(b[0:]),
			Frame:   binary.LittleEndian.Uint32(b[4:]),
			Level:   Level(b[8]),
			Channel: Channel(b[9]),
			MsgID:   MsgID(binary.LittleEndian.Uint16(b[10:])),
		}
		for i := 0; i < 4; i++ {
			e.Args[i] = int64(binary.LittleEndian.Uint64(b[16+8*i:]))
		}
		if _, err := fmt.Fprintln(bw, l.FormatEntry(&e)); err != nil {
			return err
		}
	}
}
