package telemetry

import (
	"crypto/sha256"
	"fmt"
	"os"
	"strings"
)

// CrashSignature builds an anonymous crash signature from a stack trace and the
// build hash: the first 8 bytes of the stack's SHA-256, joined to the build. It
// identifies the crash SHAPE and the build, with no file paths, symbols, or PII
// carried through (only the hash is kept).
func CrashSignature(stack, build string) string {
	h := sha256.Sum256([]byte(stack))
	return fmt.Sprintf("%x@%s", h[:8], build)
}

// WriteCrashSig persists a pending crash signature so the NEXT launch can report
// it (a crashed session cannot send for itself). The file holds only the
// signature string.
func WriteCrashSig(path, sig string) error {
	return os.WriteFile(path, []byte(sig), 0o644)
}

// TakeCrashSig reads and then REMOVES the pending crash signature, so it is
// reported at most once. Returns ("", false) when none is pending.
func TakeCrashSig(path string) (string, bool) {
	b, err := os.ReadFile(path)
	if err != nil {
		return "", false
	}
	_ = os.Remove(path) // consume — never report the same crash twice
	sig := strings.TrimSpace(string(b))
	if sig == "" {
		return "", false
	}
	return sig, true
}
