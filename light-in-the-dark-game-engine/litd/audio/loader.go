package audio

import (
	"encoding/json"
	"fmt"
	"io/fs"
	"path"
	"path/filepath"
	"sort"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio/oggmeta"
)

const (
	// StreamChunkBytes is the chunk size required by audio.md §6 for music.
	StreamChunkBytes = 64 * 1024
	// StreamRingChunks is the bounded ring depth for each streamed music asset.
	StreamRingChunks = 2
)

// LoadedAsset is one audio asset after the runtime loader has classified it.
type LoadedAsset struct {
	Path               string `json:"path"`
	Category           string `json:"category"`
	Codec              string `json:"codec"`
	Channels           int    `json:"channels"`
	SampleRate         int    `json:"sampleRate"`
	TotalSamples       int64  `json:"totalSamples"`
	DecodedBytes       int64  `json:"decodedBytes"`
	Resident           bool   `json:"resident"`
	Streamed           bool   `json:"streamed"`
	StreamChunkBytes   int    `json:"streamChunkBytes,omitempty"`
	StreamRingChunks   int    `json:"streamRingChunks,omitempty"`
	StreamBufferBytes  int    `json:"streamBufferBytes,omitempty"`
	StreamPrimedBytes  int    `json:"streamPrimedBytes,omitempty"`
	StreamDecodeOK     bool   `json:"streamDecodeOk,omitempty"`
	ResidentBufferID   int    `json:"residentBufferId,omitempty"`
	ResidentBufferSize int64  `json:"residentBufferSize,omitempty"`
	PCMDecodedBytes    int64  `json:"pcmDecodedBytes,omitempty"`
	PCMDecodeOK        bool   `json:"pcmDecodeOk,omitempty"`
}

// LoadDump is the loader source of truth for FSV: every asset, the resident
// decoded total, stream ring memory, and any fail-closed rejection.
type LoadDump struct {
	OK                   bool          `json:"ok"`
	Root                 string        `json:"root"`
	AssetCount           int           `json:"assetCount"`
	ResidentDecodedBytes int64         `json:"residentDecodedBytes"`
	BudgetBytes          int64         `json:"budgetBytes"`
	StreamBufferBytes    int           `json:"streamBufferBytes"`
	Assets               []LoadedAsset `json:"assets"`
	Errors               []string      `json:"errors,omitempty"`
}

// MarshalJSONStable returns an indented dump for issue comments and artifacts.
func (d LoadDump) MarshalJSONStable() ([]byte, error) {
	return json.MarshalIndent(d, "", "  ")
}

// LoadAssets scans root in fsys, validates every audio asset fail-closed using
// oggmeta, and returns the observable loader dump. The default implementation is
// pure Go and performs the same residency decisions as the OpenAL path: resident
// world/UI/voice assets are counted as decoded PCM buffers, while music is marked
// streamed with a bounded 64 KB chunk ring and never counted as resident.
func LoadAssets(fsys fs.FS, root string) (LoadDump, error) {
	root = cleanRoot(root)
	d := LoadDump{
		OK:          true,
		Root:        root,
		BudgetBytes: oggmeta.MaxDecodedSetBytes,
	}
	var nextResidentID int
	var errs []string

	walkErr := fs.WalkDir(fsys, root, func(p string, de fs.DirEntry, err error) error {
		if err != nil {
			errs = append(errs, fmt.Sprintf("%s: AUD-READ: %v", slash(p), err))
			return nil
		}
		if de.IsDir() {
			return nil
		}
		rel := slash(strings.TrimPrefix(slash(p), slash(root)+"/"))
		if slash(p) == slash(root) {
			rel = path.Base(slash(p))
		}
		ext := strings.ToLower(filepath.Ext(p))
		if ext != ".ogg" {
			if isRejectedAudioExt(ext) {
				errs = append(errs, fmt.Sprintf("%s: AUD-FMT: audio assets must be .ogg, got %s", rel, ext))
			}
			return nil
		}
		body, rerr := fs.ReadFile(fsys, p)
		if rerr != nil {
			errs = append(errs, fmt.Sprintf("%s: AUD-READ: %v", rel, rerr))
			return nil
		}
		info, perr := oggmeta.Parse(body)
		if perr != nil {
			errs = append(errs, fmt.Sprintf("%s: AUD-PARSE: %v", rel, perr))
			return nil
		}
		cat := oggmeta.CategoryOf(rel)
		findings, resident := oggmeta.CheckLayout(info, cat)
		for _, f := range findings {
			errs = append(errs, fmt.Sprintf("%s: %s: %s", rel, f.Rule, f.Msg))
		}
		a := LoadedAsset{
			Path:         rel,
			Category:     cat.String(),
			Codec:        info.Codec.String(),
			Channels:     info.Channels,
			SampleRate:   info.SampleRate,
			TotalSamples: info.TotalSamples,
			DecodedBytes: info.DecodedBytes(),
			Resident:     resident,
			Streamed:     cat == oggmeta.CatMusic,
		}
		if a.Streamed {
			a.StreamChunkBytes = StreamChunkBytes
			a.StreamRingChunks = StreamRingChunks
			a.StreamBufferBytes = StreamChunkBytes * StreamRingChunks
			d.StreamBufferBytes += a.StreamBufferBytes
		}
		if resident {
			nextResidentID++
			a.ResidentBufferID = nextResidentID
			a.ResidentBufferSize = a.DecodedBytes
			d.ResidentDecodedBytes += a.DecodedBytes
		}
		d.Assets = append(d.Assets, a)
		return nil
	})
	if walkErr != nil {
		errs = append(errs, fmt.Sprintf("%s: AUD-WALK: %v", root, walkErr))
	}
	sort.Slice(d.Assets, func(i, j int) bool { return d.Assets[i].Path < d.Assets[j].Path })
	d.AssetCount = len(d.Assets)
	if d.ResidentDecodedBytes > d.BudgetBytes {
		errs = append(errs, fmt.Sprintf("AUD-BUDGET: resident audio set %d bytes (%.1f MB) exceeds the %d MB per-map cap (R-AUD-1.6)",
			d.ResidentDecodedBytes, float64(d.ResidentDecodedBytes)/(1024*1024), d.BudgetBytes/(1024*1024)))
	}
	if len(errs) > 0 {
		d.OK = false
		d.Errors = errs
		return d, fmt.Errorf("audio load rejected: %s", strings.Join(errs, "; "))
	}
	return d, nil
}

func cleanRoot(root string) string {
	root = slash(strings.TrimSpace(root))
	if root == "" || root == "." {
		return "."
	}
	return strings.TrimSuffix(root, "/")
}

func slash(p string) string {
	if p == "" {
		return "."
	}
	return filepath.ToSlash(p)
}

func isRejectedAudioExt(ext string) bool {
	switch ext {
	case ".wav", ".mp3", ".flac", ".opus", ".aiff", ".aif":
		return true
	default:
		return false
	}
}
