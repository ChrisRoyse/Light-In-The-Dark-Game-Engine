package net

// #316 FSV: out-of-band, hash-isolated chat. SoT = the routing decision
// (Visible), the typed wire frames over a REAL loopback QUIC session, and the sim
// StateHash (chat-heavy vs silent runs of identical commands must be equal). The
// chat UI widget + screenshots are the GL client-shell layer (deferred — no GL
// here); everything verifiable headless is covered.

import (
	"context"
	"fmt"
	"testing"
	"time"
)

// sessionPair stands up a real loopback QUIC session pair (host, client).
func sessionPair(t *testing.T) (*Session, *Session) {
	t.Helper()
	serverTLS, clientTLS, err := SelfSignedTLS()
	if err != nil {
		t.Fatalf("SelfSignedTLS: %v", err)
	}
	ln, err := Listen("127.0.0.1:0", serverTLS)
	if err != nil {
		t.Fatalf("Listen: %v", err)
	}
	t.Cleanup(func() { ln.Close() })
	ctx, cancel := context.WithTimeout(context.Background(), 8*time.Second)
	defer cancel()
	ch := make(chan *Session, 1)
	go func() {
		s, _ := ln.Accept(ctx)
		ch <- s
	}()
	c, err := Dial(ctx, ln.Addr(), clientTLS)
	if err != nil {
		t.Fatalf("Dial: %v", err)
	}
	h := <-ch
	if h == nil {
		t.Fatal("host Accept returned nil")
	}
	return h, c
}

// TestChatRoutingMatrix — the whole visibility/info-isolation policy.
func TestChatRoutingMatrix(t *testing.T) {
	p1a := Participant{ID: 1, Team: 0}                 // team 0 player
	p1b := Participant{ID: 2, Team: 0}                 // team 0 player (ally of p1a)
	p2 := Participant{ID: 3, Team: 1}                  // team 1 player (enemy)
	obs := Participant{ID: 9, Team: 0, IsObserver: true}

	cases := []struct {
		name   string
		msg    ChatMessage
		sender Participant
		recip  Participant
		want   bool
	}{
		// (1) allies message → enemy never receives; ally + observer do.
		{"allies→ally", ChatMessage{Channel: ChanAllies, Sender: 1}, p1a, p1b, true},
		{"allies→enemy", ChatMessage{Channel: ChanAllies, Sender: 1}, p1a, p2, false},
		{"allies→observer", ChatMessage{Channel: ChanAllies, Sender: 1}, p1a, obs, true},
		// (2) observer-channel message → players never receive; observers do.
		{"obs-chan→player", ChatMessage{Channel: ChanObservers, Sender: 9}, obs, p1a, false},
		{"obs-chan→enemy", ChatMessage{Channel: ChanObservers, Sender: 9}, obs, p2, false},
		// (info isolation) observer on ALL channel → players never receive.
		{"obs-all→player", ChatMessage{Channel: ChanAll, Sender: 9}, obs, p1a, false},
		// all from a player → everyone.
		{"all→enemy", ChatMessage{Channel: ChanAll, Sender: 1}, p1a, p2, true},
		{"all→observer", ChatMessage{Channel: ChanAll, Sender: 1}, p1a, obs, true},
		// lobby → everyone present.
		{"lobby→enemy", ChatMessage{Channel: ChanLobby, Sender: 1}, p1a, p2, true},
		// no self-delivery.
		{"self", ChatMessage{Channel: ChanAll, Sender: 1}, p1a, p1a, false},
	}
	for _, c := range cases {
		if got := Visible(c.msg, c.sender, c.recip); got != c.want {
			t.Fatalf("%s: Visible=%v, want %v", c.name, got, c.want)
		}
	}
	// Recipients of an allies message among {ally, enemy, observer}: ally+observer.
	rec := Recipients(ChatMessage{Channel: ChanAllies, Sender: 1}, p1a, []Participant{p1a, p1b, p2, obs})
	if len(rec) != 2 || rec[0].ID != 2 || rec[1].ID != 9 {
		t.Fatalf("allies recipients = %+v, want [ally#2, observer#9]", rec)
	}
	t.Logf("FSV routing: enemy excluded from allies; players excluded from observer & observer-all chat; %d cases pass", len(cases))
}

// TestChatWireMultiplex — chat and turn frames share ONE reliable stream but are
// distinct, dispatchable types. Proves chat is out-of-band from the command turn.
func TestChatWireMultiplex(t *testing.T) {
	host, client := sessionPair(t)
	defer host.Close()
	defer client.Close()

	enc, err := EncodeChat(ChatMessage{Channel: ChanAll, Sender: 4, Body: "glhf 🎮"})
	if err != nil {
		t.Fatalf("EncodeChat: %v", err)
	}
	// Interleave: chat, then a command turn, then chat — over the same stream.
	if err := client.SendChat(enc); err != nil {
		t.Fatalf("SendChat: %v", err)
	}
	if err := client.SendTurn([]byte{0xAA, 0xBB}); err != nil {
		t.Fatalf("SendTurn: %v", err)
	}
	if err := client.SendChat(enc); err != nil {
		t.Fatalf("SendChat 2: %v", err)
	}

	want := []byte{KindChat, KindTurn, KindChat}
	for i, wk := range want {
		f, err := host.RecvFrame()
		if err != nil {
			t.Fatalf("RecvFrame %d: %v", i, err)
		}
		if f.Kind != wk {
			t.Fatalf("frame %d kind=%d, want %d", i, f.Kind, wk)
		}
		if f.Kind == KindChat {
			m, err := DecodeChat(f.Payload)
			if err != nil || m.Body != "glhf 🎮" || m.Sender != 4 {
				t.Fatalf("chat decode: %+v err=%v", m, err)
			}
		}
	}
	t.Logf("FSV wire: frames dispatched by kind [chat,turn,chat]; chat KindChat=%d distinct from turn KindTurn=%d (chat never enters the turn/replay stream)", KindChat, KindTurn)
}

// TestChatHashIsolationFSV — identical commands, one run silent and one flooded
// with chat over a real session, end at the SAME sim StateHash. Chat never enters
// the sim.
func TestChatHashIsolationFSV(t *testing.T) {
	gQuiet, _, _ := newTwin(t)
	gChat, unit, _ := newTwin(t)
	gateQ, _ := NewLockstepGate(2)
	gateC, _ := NewLockstepGate(2)
	host, client := sessionPair(t)
	defer host.Close()
	defer client.Close()

	aggs := [][]byte{
		turnAgg(t, stopRec(t, 1, 0, unit)),
		turnAgg(t, stopRec(t, 3, 1, unit)),
		turnAgg(t, stopRec(t, 5, 2, unit)),
	}
	sb := NewScrollback(64)
	for i, a := range aggs {
		// silent twin: just the commands.
		if err := gateQ.Deliver(uint64(i), a); err != nil {
			t.Fatal(err)
		}
		gateQ.Pump(gQuiet)
		// chatty twin: same commands, plus chat traffic over the wire each turn.
		enc, _ := EncodeChat(ChatMessage{Channel: ChanAll, Sender: 0, Body: fmt.Sprintf("turn %d!", i)})
		if err := client.SendChat(enc); err != nil {
			t.Fatalf("SendChat: %v", err)
		}
		f, err := host.RecvFrame()
		if err != nil || f.Kind != KindChat {
			t.Fatalf("recv chat: kind=%d err=%v", f.Kind, err)
		}
		m, err := DecodeChat(f.Payload)
		if err != nil {
			t.Fatalf("DecodeChat: %v", err)
		}
		sb.Add(m) // routed to scrollback — NEVER to StageCommand
		if err := gateC.Deliver(uint64(i), a); err != nil {
			t.Fatal(err)
		}
		gateC.Pump(gChat)
	}

	hq, hc := gQuiet.StateHash(), gChat.StateHash()
	if hq != hc {
		t.Fatalf("chat perturbed the sim hash: silent=%#x chatty=%#x", hq, hc)
	}
	if sb.Len() != len(aggs) {
		t.Fatalf("chat delivery: scrollback has %d, want %d (chat must be delivered, just not to the sim)", sb.Len(), len(aggs))
	}
	t.Logf("FSV hash-isolation: %d chat messages delivered; StateHash silent==chatty %#x", sb.Len(), hq)
}

// TestChatOversizeAndRateLimit — abuse paths fail closed; the session stays healthy.
func TestChatOversizeAndRateLimit(t *testing.T) {
	// Oversized body rejected at encode (nothing reaches the wire).
	big := make([]byte, MaxChatBytes+1)
	for i := range big {
		big[i] = 'a'
	}
	if _, err := EncodeChat(ChatMessage{Channel: ChanAll, Body: string(big)}); err == nil {
		t.Fatal("oversized chat body accepted")
	}

	// Rate flood: a 3-token bucket allows 3, refuses the 4th, then recovers.
	rl := NewRateLimiter(3, 1) // 3 cap, 1/sec refill
	base := time.Unix(1000, 0)
	allowed := 0
	for i := 0; i < 5; i++ {
		if rl.Allow(base) { // same instant → no refill
			allowed++
		}
	}
	if allowed != 3 {
		t.Fatalf("burst allowed %d, want 3 (bucket cap)", allowed)
	}
	if !rl.Allow(base.Add(1100 * time.Millisecond)) {
		t.Fatal("after 1.1s refill, a message should be allowed")
	}
	t.Logf("FSV abuse: oversized body refused at encode; rate bucket allowed 3 burst + 1 after refill")

	// Session stays healthy: after the rejected encode, a valid chat round-trips.
	host, client := sessionPair(t)
	defer host.Close()
	defer client.Close()
	ok, err := EncodeChat(ChatMessage{Channel: ChanAll, Sender: 1, Body: "still works"})
	if err != nil {
		t.Fatalf("EncodeChat valid: %v", err)
	}
	if err := client.SendChat(ok); err != nil {
		t.Fatalf("SendChat after rejection: %v", err)
	}
	f, err := host.RecvFrame()
	if err != nil || f.Kind != KindChat {
		t.Fatalf("recv after rejection: kind=%d err=%v", f.Kind, err)
	}
	if m, _ := DecodeChat(f.Payload); m.Body != "still works" {
		t.Fatalf("post-rejection chat body=%q", m.Body)
	}
	t.Log("FSV health: session usable after a rejected message")
}

// TestChatDecodeRefusesMalformed — non-plain-text / malformed frames are dropped.
func TestChatDecodeRefusesMalformed(t *testing.T) {
	// Invalid UTF-8 and control characters refused at encode.
	if _, err := EncodeChat(ChatMessage{Channel: ChanAll, Body: "bad\x00null"}); err == nil {
		t.Fatal("control char accepted")
	}
	if _, err := EncodeChat(ChatMessage{Channel: ChanAll, Body: "line\nbreak"}); err == nil {
		t.Fatal("newline accepted")
	}
	if _, err := EncodeChat(ChatMessage{Channel: ChanAll, Body: string([]byte{0xff, 0xfe})}); err == nil {
		t.Fatal("invalid UTF-8 accepted")
	}
	// Malformed wire frames refused at decode.
	for _, bad := range [][]byte{
		nil,
		{0x00},                         // truncated
		{0xFF, 0x00, 0x00, 0x00},       // invalid channel
		{0x01, 0x00, 0x09, 0x00, 'h'},  // length 9 but 1 body byte
		append([]byte{0x01, 0x00, 0x02, 0x00, 'h', 'i'}, 'X'), // trailing byte
	} {
		if _, err := DecodeChat(bad); err == nil {
			t.Fatalf("DecodeChat accepted malformed %v", bad)
		}
	}
	t.Log("FSV malformed: control/newline/invalid-UTF-8 bodies + 5 malformed wire frames all refused")
}
