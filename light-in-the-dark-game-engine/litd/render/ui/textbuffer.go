package ui

// defaultTextBufferBytes bounds a single overlay text line. Lines longer than
// this are truncated at commit — overlays are short tokens, never prose.
const defaultTextBufferBytes = 128

// TextBuffer is a fixed-capacity byte buffer for one rendered overlay line,
// reused frame to frame so painting allocates nothing (R-GC-1). Mirrors the
// hud package's buffer; kept local so the ui package has no hud dependency.
type TextBuffer struct {
	buf [defaultTextBufferBytes]byte
	n   int
}

func (b *TextBuffer) Bytes() []byte  { return b.buf[:b.n] }
func (b *TextBuffer) String() string { return string(b.Bytes()) }

// reset returns the backing slice truncated to zero length for re-appending.
func (b *TextBuffer) reset() []byte { b.n = 0; return b.buf[:0] }

// commit records the new length, clamping to capacity (truncating overflow).
func (b *TextBuffer) commit(p []byte) {
	if len(p) > len(b.buf) {
		b.n = len(b.buf)
		return
	}
	b.n = len(p)
}
