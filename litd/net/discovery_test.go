package net

// #63 FSV: LAN discovery. SoT = the discoverer's session table. A real loopback
// UDP round-trip proves encode→socket→decode→table; the multi-host/expiry/garbage
// edges use the real table logic with an INJECTED clock (deterministic, no
// multi-second waits).

import (
	"net"
	"testing"
	"time"
)

func beaconBytes(t *testing.T, info BeaconInfo) []byte {
	t.Helper()
	b, err := EncodeBeacon(info)
	if err != nil {
		t.Fatalf("EncodeBeacon: %v", err)
	}
	return b
}

func TestBeaconCodecRoundTrip(t *testing.T) {
	info := BeaconInfo{HostAddr: "192.168.1.7:7777", SessionName: "Vigil vs Unbound", Players: 3, Capacity: 8, BuildHash: "litd-0.1.0+deadbeef"}
	back, err := DecodeBeacon(beaconBytes(t, info))
	if err != nil {
		t.Fatalf("DecodeBeacon: %v", err)
	}
	if back != info {
		t.Fatalf("round-trip mismatch:\n got %+v\n want %+v", back, info)
	}
	// Malformed inputs are non-fatal errors.
	for _, bad := range [][]byte{
		nil,
		[]byte("XXXX"),                 // bad magic
		[]byte("LDB1\x00"),             // truncated
		append([]byte("LDB1"), 0xFF, 0xFF), // claims 65535-byte string
	} {
		if _, err := DecodeBeacon(bad); err == nil {
			t.Fatalf("DecodeBeacon accepted malformed packet %q", bad)
		}
	}
	t.Logf("FSV codec: beacon round-trips; 4 malformed packets all refused")
}

// TestDiscoveryLoopbackWire — a real Beacon packet traverses a UDP socket and
// lands in the discoverer table.
func TestDiscoveryLoopbackWire(t *testing.T) {
	rx, err := net.ListenUDP("udp", &net.UDPAddr{IP: net.IPv4(127, 0, 0, 1), Port: 0})
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	defer rx.Close()
	port := rx.LocalAddr().(*net.UDPAddr).Port

	info := BeaconInfo{HostAddr: "127.0.0.1:7000", SessionName: "loopback", Players: 1, Capacity: 4, BuildHash: "b1"}
	b, err := NewBeacon(info, net.JoinHostPort("127.0.0.1", itoa(port)))
	if err != nil {
		t.Fatalf("NewBeacon: %v", err)
	}
	defer b.Close()
	if err := b.Send(); err != nil {
		t.Fatalf("Send: %v", err)
	}

	d := NewDiscoverer(DefaultEntryTTL)
	buf := make([]byte, maxBeaconWire)
	_ = rx.SetReadDeadline(time.Now().Add(3 * time.Second))
	n, _, err := rx.ReadFromUDP(buf)
	if err != nil {
		t.Fatalf("ReadFromUDP: %v", err)
	}
	now := time.Unix(1000, 0)
	if !d.HandlePacket(buf[:n], now) {
		t.Fatal("HandlePacket rejected a valid beacon")
	}
	got := d.Sessions(now)
	if len(got) != 1 || got[0] != info {
		t.Fatalf("table=%+v, want [%+v]", got, info)
	}
	t.Logf("FSV loopback wire: received %d B → table lists %+v", n, got[0])
}

// TestDiscoveryTwoHostsAndExpiry — two hosts both listed; a host that stops
// beaconing expires after the TTL while the other survives.
func TestDiscoveryTwoHostsAndExpiry(t *testing.T) {
	d := NewDiscoverer(5 * time.Second)
	t0 := time.Unix(2000, 0)

	hostA := BeaconInfo{HostAddr: "127.0.0.1:5001", SessionName: "A", Players: 1, Capacity: 2, BuildHash: "b"}
	hostB := BeaconInfo{HostAddr: "127.0.0.1:5002", SessionName: "B", Players: 2, Capacity: 4, BuildHash: "b"}
	d.HandlePacket(beaconBytes(t, hostA), t0)
	d.HandlePacket(beaconBytes(t, hostB), t0)

	at0 := d.Sessions(t0)
	if len(at0) != 2 || at0[0] != hostA || at0[1] != hostB {
		t.Fatalf("t0 table=%+v, want [A,B]", at0)
	}
	t.Logf("FSV t=0: both hosts listed: %s, %s", at0[0].HostAddr, at0[1].HostAddr)

	// A keeps beaconing at t0+3; B goes silent.
	d.HandlePacket(beaconBytes(t, hostA), t0.Add(3*time.Second))

	// At t0+6: B (last seen t0, 6s > 5s TTL) expires; A (last seen t0+3) survives.
	at6 := d.Sessions(t0.Add(6 * time.Second))
	if len(at6) != 1 || at6[0] != hostA {
		t.Fatalf("t0+6 table=%+v, want [A] (B expired)", at6)
	}
	t.Logf("FSV t=6s: B expired, only %s remains", at6[0].HostAddr)
}

// TestDiscoveryGarbageIgnored — a garbage UDP packet never alters the table.
func TestDiscoveryGarbageIgnored(t *testing.T) {
	d := NewDiscoverer(5 * time.Second)
	now := time.Unix(3000, 0)
	host := BeaconInfo{HostAddr: "127.0.0.1:6001", SessionName: "live", Players: 1, Capacity: 2, BuildHash: "b"}
	d.HandlePacket(beaconBytes(t, host), now)

	before := d.Sessions(now)
	t.Logf("FSV before garbage: table=%d entry(s) (%s)", len(before), before[0].HostAddr)

	for _, garbage := range [][]byte{
		[]byte("not a beacon"),
		{0x00, 0x01, 0x02, 0x03, 0x04},
		[]byte("LDB1junk"),
	} {
		if d.HandlePacket(garbage, now) {
			t.Fatalf("garbage packet %q updated the table", garbage)
		}
	}
	after := d.Sessions(now)
	if len(after) != 1 || after[0] != host {
		t.Fatalf("table changed after garbage: %+v", after)
	}
	t.Logf("FSV after garbage: table unchanged (%d entry, %s)", len(after), after[0].HostAddr)
}

// itoa avoids strconv import churn for the one port conversion.
func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	var buf [6]byte
	i := len(buf)
	for n > 0 {
		i--
		buf[i] = byte('0' + n%10)
		n /= 10
	}
	return string(buf[i:])
}
