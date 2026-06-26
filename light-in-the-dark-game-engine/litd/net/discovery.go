package net

// discovery.go: LAN UDP broadcast discovery (#63, D-2026-06-11-26 LAN star).
// A host BEACONS a small UDP packet describing its joinable session; clients
// LISTEN and maintain a table of currently-advertised sessions, expiring an
// entry a few seconds after its last beacon. This is presentation-side only —
// nothing here crosses the determinism boundary (the discovered addr feeds the
// QUIC Dial/join flow, not the sim).
//
// Robustness: a malformed/garbage packet on the discovery port is ignored, never
// fatal (a noisy LAN must not crash the lobby). Expiry is computed against a
// caller-supplied "now", so the table logic is deterministically testable
// without real multi-second waits.

import (
	"encoding/binary"
	"fmt"
	"net"
	"sort"
	"time"
)

const (
	// DefaultBeaconInterval is how often a host re-advertises (D-2026-06-11-26).
	DefaultBeaconInterval = 1 * time.Second
	// DefaultEntryTTL is how long after the last beacon an entry survives.
	DefaultEntryTTL = 5 * time.Second

	beaconMagic   = "LDB1" // litd discovery beacon v1
	maxBeaconWire = 1024    // total packet cap (fail-closed)
	maxStrWire    = 255     // per-string field cap
)

// BeaconInfo is the advertised session description.
type BeaconInfo struct {
	HostAddr    string // host:port to QUIC-dial
	SessionName string
	Players     uint8
	Capacity    uint8
	BuildHash   string // join guard cross-checks this (#74)
}

// EncodeBeacon serializes info to a discovery packet. Fail-closed on an
// over-long field.
func EncodeBeacon(info BeaconInfo) ([]byte, error) {
	for _, s := range []string{info.HostAddr, info.SessionName, info.BuildHash} {
		if len(s) > maxStrWire {
			return nil, fmt.Errorf("net: beacon field too long: %d > %d", len(s), maxStrWire)
		}
	}
	out := make([]byte, 0, 64)
	out = append(out, beaconMagic...)
	out = appendStr(out, info.HostAddr)
	out = appendStr(out, info.SessionName)
	out = append(out, info.Players, info.Capacity)
	out = appendStr(out, info.BuildHash)
	return out, nil
}

// DecodeBeacon parses a discovery packet. A bad magic, truncation, or over-cap
// field is a (non-fatal) error so the listener can ignore it.
func DecodeBeacon(b []byte) (BeaconInfo, error) {
	if len(b) > maxBeaconWire {
		return BeaconInfo{}, fmt.Errorf("net: beacon too large: %d", len(b))
	}
	if len(b) < len(beaconMagic) || string(b[:len(beaconMagic)]) != beaconMagic {
		return BeaconInfo{}, fmt.Errorf("net: beacon bad magic")
	}
	p := len(beaconMagic)
	host, p, err := readStr(b, p)
	if err != nil {
		return BeaconInfo{}, err
	}
	name, p, err := readStr(b, p)
	if err != nil {
		return BeaconInfo{}, err
	}
	if p+2 > len(b) {
		return BeaconInfo{}, fmt.Errorf("net: beacon truncated at counts")
	}
	players, capacity := b[p], b[p+1]
	p += 2
	build, p, err := readStr(b, p)
	if err != nil {
		return BeaconInfo{}, err
	}
	if p != len(b) {
		return BeaconInfo{}, fmt.Errorf("net: beacon has %d trailing bytes", len(b)-p)
	}
	return BeaconInfo{HostAddr: host, SessionName: name, Players: players, Capacity: capacity, BuildHash: build}, nil
}

func appendStr(dst []byte, s string) []byte {
	var l [2]byte
	binary.BigEndian.PutUint16(l[:], uint16(len(s)))
	dst = append(dst, l[:]...)
	return append(dst, s...)
}

func readStr(b []byte, p int) (string, int, error) {
	if p+2 > len(b) {
		return "", p, fmt.Errorf("net: beacon truncated length")
	}
	n := int(binary.BigEndian.Uint16(b[p : p+2]))
	p += 2
	if n > maxStrWire || p+n > len(b) {
		return "", p, fmt.Errorf("net: beacon string length %d invalid", n)
	}
	return string(b[p : p+n]), p + n, nil
}

// Discoverer maintains the live table of advertised sessions. Not safe for
// concurrent use; the Listen goroutine and Sessions readers are serialized by
// the caller, or call HandlePacket from one goroutine.
type Discoverer struct {
	ttl   time.Duration
	table map[string]discEntry // keyed by HostAddr
}

type discEntry struct {
	info     BeaconInfo
	lastSeen time.Time
}

// NewDiscoverer builds a discoverer with the given entry TTL (0 → default).
func NewDiscoverer(ttl time.Duration) *Discoverer {
	if ttl <= 0 {
		ttl = DefaultEntryTTL
	}
	return &Discoverer{ttl: ttl, table: make(map[string]discEntry)}
}

// HandlePacket decodes one received packet and, if valid, upserts its session
// into the table with lastSeen=now. A malformed packet is ignored (returns
// false) — never fatal. Returns true if the table was updated.
func (d *Discoverer) HandlePacket(data []byte, now time.Time) bool {
	info, err := DecodeBeacon(data)
	if err != nil || info.HostAddr == "" {
		return false
	}
	d.table[info.HostAddr] = discEntry{info: info, lastSeen: now}
	return true
}

// Sessions returns the non-expired advertised sessions as of now, sorted by host
// address (deterministic ordering for display). Expired entries are pruned.
func (d *Discoverer) Sessions(now time.Time) []BeaconInfo {
	out := make([]BeaconInfo, 0, len(d.table))
	for addr, e := range d.table {
		if now.Sub(e.lastSeen) > d.ttl {
			delete(d.table, addr)
			continue
		}
		out = append(out, e.info)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].HostAddr < out[j].HostAddr })
	return out
}

// Beacon broadcasts a session's BeaconInfo on a UDP socket. The target is the
// LAN broadcast address in production; tests target a specific loopback addr.
type Beacon struct {
	conn   *net.UDPConn
	target *net.UDPAddr
	info   BeaconInfo
}

// NewBeacon dials a UDP socket toward target (e.g. "255.255.255.255:PORT" on a
// LAN, or "127.0.0.1:PORT" in tests) for advertising info.
func NewBeacon(info BeaconInfo, target string) (*Beacon, error) {
	addr, err := net.ResolveUDPAddr("udp", target)
	if err != nil {
		return nil, fmt.Errorf("net: beacon target %q: %w", target, err)
	}
	conn, err := net.DialUDP("udp", nil, addr)
	if err != nil {
		return nil, fmt.Errorf("net: beacon dial %q: %w", target, err)
	}
	return &Beacon{conn: conn, target: addr, info: info}, nil
}

// Send emits one beacon packet.
func (b *Beacon) Send() error {
	pkt, err := EncodeBeacon(b.info)
	if err != nil {
		return err
	}
	if _, err := b.conn.Write(pkt); err != nil {
		return fmt.Errorf("net: beacon send: %w", err)
	}
	return nil
}

// Close releases the beacon socket.
func (b *Beacon) Close() error { return b.conn.Close() }
