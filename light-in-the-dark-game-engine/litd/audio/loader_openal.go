//go:build openal

package audio

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"unsafe"

	g3naudio "github.com/g3n/engine/audio"
)

// LoadRuntimeAssetsDir loads and validates audio assets from an OS directory,
// then uses G3N/libvorbis to prove resident assets decode to PCM bytes and
// streamed music primes only the bounded 64 KB ring chunk.
func LoadRuntimeAssetsDir(dir string) (LoadDump, error) {
	dir = filepath.Clean(dir)
	parent := filepath.Dir(dir)
	root := filepath.Base(dir)
	d, err := LoadAssets(os.DirFS(parent), root)
	if err != nil {
		return d, err
	}

	var errs []string
	for i := range d.Assets {
		asset := &d.Assets[i]
		full := filepath.Join(dir, filepath.FromSlash(asset.Path))
		switch {
		case asset.Resident:
			pcm, derr := decodePCMFile(full, asset.DecodedBytes)
			n := int64(len(pcm))
			asset.PCMDecodedBytes = n
			if derr != nil {
				errs = append(errs, fmt.Sprintf("%s: AUD-DECODE: %v", asset.Path, derr))
				continue
			}
			if n != asset.DecodedBytes {
				errs = append(errs, fmt.Sprintf("%s: AUD-DECODE: decoded %d PCM bytes, metadata expected %d", asset.Path, n, asset.DecodedBytes))
				continue
			}
			asset.PCMDecodeOK = true
		case asset.Streamed:
			n, derr := primeStreamChunk(full)
			asset.StreamPrimedBytes = n
			if derr != nil {
				errs = append(errs, fmt.Sprintf("%s: AUD-STREAM: %v", asset.Path, derr))
				continue
			}
			if n > StreamChunkBytes {
				errs = append(errs, fmt.Sprintf("%s: AUD-STREAM: primed %d bytes, exceeds chunk %d", asset.Path, n, StreamChunkBytes))
				continue
			}
			asset.StreamDecodeOK = true
		}
	}
	if len(errs) > 0 {
		d.OK = false
		d.Errors = append(d.Errors, errs...)
		return d, fmt.Errorf("audio decode rejected: %s", strings.Join(errs, "; "))
	}
	return d, nil
}

func decodePCMFile(filename string, expectedBytes int64) ([]byte, error) {
	if expectedBytes < 0 {
		return nil, fmt.Errorf("negative expected PCM byte count %d", expectedBytes)
	}
	if expectedBytes > int64(int(^uint(0)>>1)) {
		return nil, fmt.Errorf("expected PCM byte count %d exceeds addressable memory", expectedBytes)
	}
	af, err := g3naudio.NewAudioFile(filename)
	if err != nil {
		return nil, err
	}
	defer af.Close()

	buf := make([]byte, StreamChunkBytes)
	out := make([]byte, 0, int(expectedBytes))
	for {
		n, rerr := af.Read(unsafe.Pointer(&buf[0]), len(buf))
		if n > 0 {
			if expectedBytes > 0 && int64(len(out)+n) > expectedBytes {
				return out, fmt.Errorf("decoder produced more than metadata expected (%d > %d bytes)", len(out)+n, expectedBytes)
			}
			out = append(out, buf[:n]...)
		}
		if rerr == io.EOF {
			return out, nil
		}
		if rerr != nil {
			return out, rerr
		}
		if n == 0 {
			return out, fmt.Errorf("decoder returned 0 bytes without EOF")
		}
	}
}

func primeStreamChunk(filename string) (int, error) {
	af, err := g3naudio.NewAudioFile(filename)
	if err != nil {
		return 0, err
	}
	defer af.Close()

	buf := make([]byte, StreamChunkBytes)
	n, rerr := af.Read(unsafe.Pointer(&buf[0]), len(buf))
	if rerr != nil && rerr != io.EOF {
		return n, rerr
	}
	return n, nil
}
