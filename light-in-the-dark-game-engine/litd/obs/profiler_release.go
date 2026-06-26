//go:build !litddev

package obs

// profiler_release.go is the default (release) build: it imports neither
// net/http/pprof nor runtime/trace, so those symbols are absent from the shipped
// binary (verify with `go tool nm <bin> | grep pprof` → empty). StartProfiler is
// a hard, loud no-op: the endpoints simply do not exist in a release build.

// DevProfilerAvailable reports whether this binary was built with the profiler.
// false in a release build.
const DevProfilerAvailable = false

// StartProfiler always fails in a release build — the profiling endpoints are
// compiled out. Rebuild with -tags litddev to enable them.
func StartProfiler(addr string) (Profiler, error) {
	return nil, ErrProfilerUnavailable
}
