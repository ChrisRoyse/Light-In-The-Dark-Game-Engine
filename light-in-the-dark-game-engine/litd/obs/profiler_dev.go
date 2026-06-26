//go:build litddev

package obs

// profiler_dev.go is compiled only under the litddev build tag. It imports
// net/http/pprof for its handler funcs (NOT for the DefaultServeMux side effect
// — handlers are mounted on a private mux) and serves them on a localhost-only
// listener. Release builds compile profiler_release.go instead, so none of
// these symbols reach the shipped binary.

import (
	"context"
	"fmt"
	"net"
	"net/http"
	"net/http/pprof"
	"time"
)

// DevProfilerAvailable reports whether this binary was built with the profiler
// (true under -tags litddev).
const DevProfilerAvailable = true

type devProfiler struct {
	srv *http.Server
	ln  net.Listener
}

func (p *devProfiler) Addr() string { return p.ln.Addr().String() }

func (p *devProfiler) Close() error {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	return p.srv.Shutdown(ctx)
}

// StartProfiler binds a localhost-only HTTP server exposing the standard
// net/http/pprof + runtime/trace endpoints under /debug/pprof/, and starts
// serving in the background. addr is a host:port; a non-loopback host is
// refused (ErrProfilerNotLoopback). Use ":0" / "127.0.0.1:0" to get an
// OS-assigned port (read back via Addr).
func StartProfiler(addr string) (Profiler, error) {
	host, _, err := net.SplitHostPort(addr)
	if err != nil {
		return nil, fmt.Errorf("litd/obs: bad profiler addr %q: %w", addr, err)
	}
	if !loopbackHost(host) {
		return nil, ErrProfilerNotLoopback
	}
	if host == "" {
		_, port, _ := net.SplitHostPort(addr)
		addr = net.JoinHostPort("127.0.0.1", port)
	}
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		return nil, fmt.Errorf("litd/obs: profiler listen: %w", err)
	}

	mux := http.NewServeMux()
	mux.HandleFunc("/debug/pprof/", pprof.Index)
	mux.HandleFunc("/debug/pprof/cmdline", pprof.Cmdline)
	mux.HandleFunc("/debug/pprof/profile", pprof.Profile)
	mux.HandleFunc("/debug/pprof/symbol", pprof.Symbol)
	mux.HandleFunc("/debug/pprof/trace", pprof.Trace)

	srv := &http.Server{Handler: mux, ReadHeaderTimeout: 5 * time.Second}
	p := &devProfiler{srv: srv, ln: ln}
	go srv.Serve(ln) //nolint:errcheck // Serve returns ErrServerClosed on Close
	return p, nil
}
