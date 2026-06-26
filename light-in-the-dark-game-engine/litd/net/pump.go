package net

import "time"

// peerPump owns the single read goroutine for one peer's command-turn stream so a
// deadlined collect round (#71) can poll for the next turn WITHOUT ever issuing a
// second concurrent RecvTurn — the Session forbids concurrent reads on a stream
// (session.go: "concurrent RecvTurns are not supported"). Today Host.CollectRound
// calls RecvTurn inline and blocks the whole round on the slowest peer, then
// instant-drops on any read error with no grace. The pump is the missing seam: it
// turns the blocking read into a poll with a timeout, so the round loop can notice
// a peer is lagging, pause, and consult the StallController's grace policy instead
// of either hanging forever or dropping a merely-slow-but-alive peer.
//
// One read is outstanding at a time. Poll starts the read goroutine if none is
// running; on timeout it leaves that goroutine running (the read stays
// outstanding) and reports PumpPending — it never launches a second read. When the
// turn finally lands it sits in the 1-buffered channel and the next Poll returns it
// immediately. A read error (peer gone / stream closed) latches PumpClosed
// permanently (fail-closed: a closed peer never spuriously reports a turn).
//
// This is a pure, IO-over-an-injected-stream primitive with no production caller
// yet — same bottom-up idiom as DelayController / StallController.

// pumpStatus is the outcome of a Poll.
type pumpStatus uint8

const (
	// PumpPending: no turn arrived within the timeout. The peer is still alive and
	// the read is still outstanding — do NOT call RecvTurn elsewhere on this peer.
	PumpPending pumpStatus = iota
	// PumpReady: a turn payload is available (the []byte from Poll).
	PumpReady
	// PumpClosed: the stream errored or closed; the peer is gone. Latched.
	PumpClosed
)

func (s pumpStatus) String() string {
	switch s {
	case PumpPending:
		return "pending"
	case PumpReady:
		return "ready"
	case PumpClosed:
		return "closed"
	default:
		return "unknown"
	}
}

type pumpResult struct {
	payload []byte
	err     error
}

type peerPump struct {
	sess    *Session
	results chan pumpResult
	pending bool // a read goroutine is outstanding, its result not yet consumed
	closed  bool // latched once the stream has errored/closed
}

// newPeerPump wraps a Session. It starts no goroutine until the first Poll.
func newPeerPump(sess *Session) *peerPump {
	return &peerPump{sess: sess, results: make(chan pumpResult, 1)}
}

// Poll returns the next turn if one is ready within timeout. It is NOT safe for
// concurrent use on one pump — the round loop polls each peer's pump serially. A
// timeout of 0 polls once without blocking (still starts the read if none is
// outstanding, so the turn can be picked up by a later Poll).
func (p *peerPump) Poll(timeout time.Duration) ([]byte, pumpStatus) {
	if p.closed {
		return nil, PumpClosed
	}
	if !p.pending {
		p.pending = true
		go func(s *Session, out chan<- pumpResult) {
			b, err := s.RecvTurn()
			out <- pumpResult{payload: b, err: err}
		}(p.sess, p.results)
	}

	timer := time.NewTimer(timeout)
	defer timer.Stop()
	select {
	case r := <-p.results:
		p.pending = false
		if r.err != nil {
			p.closed = true
			return nil, PumpClosed
		}
		return r.payload, PumpReady
	case <-timer.C:
		// Read still outstanding; leave p.pending true so the next Poll waits on
		// the SAME goroutine instead of starting a second concurrent read.
		return nil, PumpPending
	}
}

// Closed reports whether the pump has latched closed.
func (p *peerPump) Closed() bool { return p.closed }

// Close releases the underlying session. An outstanding read goroutine unblocks
// when the stream tears down and its result is dropped (the buffered channel
// accepts it without a consumer).
func (p *peerPump) Close() error { return p.sess.Close() }
