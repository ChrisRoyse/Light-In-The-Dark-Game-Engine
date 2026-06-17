package obs

// profiler.go is the build-tag-gated dev profiling surface (R-OBS-5,
// observability-and-debugging.md §5): dev builds expose net/http/pprof +
// runtime/trace on a localhost-only HTTP server; release builds compile a stub
// that imports neither, so the pprof/trace symbols are absent from the shipped
// binary (verifiable via `go tool nm`). The split lives in profiler_dev.go
// (//go:build litddev) and profiler_release.go (//go:build !litddev); the shared
// contract is here.

import (
	"errors"
	"net"
)

// ErrProfilerUnavailable is returned by StartProfiler in a release build (no
// litddev tag): the profiling endpoints are intentionally absent.
var ErrProfilerUnavailable = errors.New("litd/obs: profiler unavailable in release build (rebuild with -tags litddev)")

// ErrProfilerNotLoopback is returned when StartProfiler is asked to bind a
// non-loopback address — the profiler is localhost-only, never network-exposed.
var ErrProfilerNotLoopback = errors.New("litd/obs: profiler bind address must be loopback (127.0.0.1 / ::1 / localhost)")

// Profiler is a running dev profiling server. Close stops it.
type Profiler interface {
	// Addr is the actual listen address (host:port), with the OS-assigned port
	// resolved when ":0" was requested.
	Addr() string
	// Close shuts the server down.
	Close() error
}

// loopbackHost reports whether host names the loopback interface. An empty host
// (e.g. ":6060") is treated as loopback because the dev binder rewrites it to
// 127.0.0.1 before listening.
func loopbackHost(host string) bool {
	switch host {
	case "", "localhost":
		return true
	}
	ip := net.ParseIP(host)
	return ip != nil && ip.IsLoopback()
}
