package net

// join.go: the lockstep JOIN GUARD (#74, D-2026-06-11-26). After PSK auth (#61)
// and before lobby entry, the joining client and host exchange a build hash and
// the map/PRNG seed. Any mismatch refuses the session with a reason that
// DISTINGUISHES a build-mismatch from a seed-mismatch, sent back verbatim so the
// client can show the user exactly why. A refused client never receives turn
// data — the guard runs on the established stream before any SendTurn/RecvTurn,
// and the caller closes the session on the returned error.
//
// Why a guard at all: lockstep determinism (R-FSV-2) holds only if every peer
// runs the SAME binary against the SAME seed. A divergent build or seed would
// desync silently on turn 1; catching it at join turns a mystery desync into a
// clear, actionable refusal. buildHash is caller-supplied (the release pipeline
// stamps it — the same value the update manifest uses), so litd/net stays
// decoupled from how the version is computed.

import (
	"encoding/binary"
	"fmt"
	"io"
)

// Join verdict codes, sent host→client in the join response.
const (
	joinAccept        uint8 = 0
	joinBuildMismatch uint8 = 1
	joinSeedMismatch  uint8 = 2
)

const (
	maxBuildWire   = 128 // build-hash byte cap on the wire (fail-closed alloc bound)
	maxJoinMsgWire = 256 // refusal-reason byte cap
)

func joinCodeName(code uint8) string {
	switch code {
	case joinAccept:
		return "accept"
	case joinBuildMismatch:
		return "build-mismatch"
	case joinSeedMismatch:
		return "seed-mismatch"
	default:
		return fmt.Sprintf("unknown(%d)", code)
	}
}

// ClientJoin sends this client's buildHash + seed and waits for the host's
// verdict. On refusal it returns an error carrying the host's verbatim reason
// (build- vs seed-mismatch). Call once, right after DialAuthenticated and before
// any SendTurn.
func (s *Session) ClientJoin(buildHash string, seed uint64) error {
	if err := writeJoinRequest(s.stream, buildHash, seed); err != nil {
		return fmt.Errorf("net: join: send request: %w", err)
	}
	code, msg, err := readJoinResponse(s.stream)
	if err != nil {
		return fmt.Errorf("net: join: read host verdict: %w", err)
	}
	if code != joinAccept {
		return fmt.Errorf("net: join refused by host [%s]: %s", joinCodeName(code), msg)
	}
	return nil
}

// HostJoinGuard reads the joining client's buildHash + seed, compares to the
// host's own, and replies with a verdict. On mismatch it sends the distinguished
// reason and returns an error; on a malformed/truncated request it returns an
// error WITHOUT crashing (the host accept loop keeps running for other peers).
// The caller closes the session on any returned error. Call once, right after
// AcceptAuthenticated and before any RecvTurn.
func (s *Session) HostJoinGuard(buildHash string, seed uint64) error {
	req, err := readJoinRequest(s.stream)
	if err != nil {
		return fmt.Errorf("net: join guard: malformed join request: %w", err)
	}
	if req.build != buildHash {
		msg := fmt.Sprintf("build-mismatch: host=%q client=%q", buildHash, req.build)
		_ = writeJoinResponse(s.stream, joinBuildMismatch, msg)
		return fmt.Errorf("net: join refused: %s", msg)
	}
	if req.seed != seed {
		msg := fmt.Sprintf("seed-mismatch: host=%d client=%d", seed, req.seed)
		_ = writeJoinResponse(s.stream, joinSeedMismatch, msg)
		return fmt.Errorf("net: join refused: %s", msg)
	}
	if err := writeJoinResponse(s.stream, joinAccept, "ok"); err != nil {
		return fmt.Errorf("net: join guard: accept reply: %w", err)
	}
	return nil
}

type joinRequest struct {
	build string
	seed  uint64
}

func writeJoinRequest(w io.Writer, build string, seed uint64) error {
	if len(build) > maxBuildWire {
		return fmt.Errorf("net: build hash too long: %d > %d", len(build), maxBuildWire)
	}
	buf := make([]byte, 2+len(build)+8)
	binary.BigEndian.PutUint16(buf[:2], uint16(len(build)))
	copy(buf[2:2+len(build)], build)
	binary.BigEndian.PutUint64(buf[2+len(build):], seed)
	_, err := w.Write(buf)
	return err
}

func readJoinRequest(r io.Reader) (joinRequest, error) {
	var lenb [2]byte
	if _, err := io.ReadFull(r, lenb[:]); err != nil {
		return joinRequest{}, err
	}
	n := binary.BigEndian.Uint16(lenb[:])
	if int(n) > maxBuildWire {
		return joinRequest{}, fmt.Errorf("build hash length %d exceeds %d cap", n, maxBuildWire)
	}
	buf := make([]byte, int(n)+8)
	if _, err := io.ReadFull(r, buf); err != nil {
		return joinRequest{}, err
	}
	return joinRequest{build: string(buf[:n]), seed: binary.BigEndian.Uint64(buf[n:])}, nil
}

func writeJoinResponse(w io.Writer, code uint8, msg string) error {
	if len(msg) > maxJoinMsgWire {
		msg = msg[:maxJoinMsgWire]
	}
	buf := make([]byte, 3+len(msg))
	buf[0] = code
	binary.BigEndian.PutUint16(buf[1:3], uint16(len(msg)))
	copy(buf[3:], msg)
	_, err := w.Write(buf)
	return err
}

func readJoinResponse(r io.Reader) (code uint8, msg string, err error) {
	var head [3]byte
	if _, err = io.ReadFull(r, head[:]); err != nil {
		return 0, "", err
	}
	code = head[0]
	n := binary.BigEndian.Uint16(head[1:3])
	if int(n) > maxJoinMsgWire {
		return 0, "", fmt.Errorf("join response length %d exceeds %d cap", n, maxJoinMsgWire)
	}
	m := make([]byte, n)
	if _, err = io.ReadFull(r, m); err != nil {
		return 0, "", err
	}
	return code, string(m), nil
}
