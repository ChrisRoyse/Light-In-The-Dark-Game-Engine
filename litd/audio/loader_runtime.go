//go:build !openal

package audio

import (
	"os"
	"path/filepath"
)

// LoadRuntimeAssetsDir loads audio assets from an OS directory for runtime
// inspection. In the default pure-Go build this performs metadata, layout, and
// residency validation only; the openal build tag adds real Vorbis PCM decode.
func LoadRuntimeAssetsDir(dir string) (LoadDump, error) {
	dir = filepath.Clean(dir)
	parent := filepath.Dir(dir)
	root := filepath.Base(dir)
	return LoadAssets(os.DirFS(parent), root)
}
