// Package oggmeta is the pure-Go Ogg/Vorbis metadata parser + audio layout gate
// (#228, audio.md §3 R-AUD-1.2, §6 R-AUD-1.6). It reads the Ogg page framing and
// the codec identification header to recover codec / channel count / sample rate /
// total samples WITHOUT decoding the bitstream (no libvorbis, no cgo), so both the
// build-time validator (tools/assetcheck) and the in-engine loader share one set of
// channel-layout + codec + decoded-size rules.
//
// It deliberately does NOT validate the per-page CRC or the Vorbis setup/codebooks
// — bitstream integrity is the decoder's job (defense in depth: structural + codec
// checks here, full decode validation at load). It fails closed: any structural or
// codec anomaly is an error, never a silently-skipped check.
package oggmeta

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"io"
	"strings"
)

// Codec identifies the bitstream inside the Ogg container.
type Codec uint8

const (
	CodecUnknown Codec = iota
	CodecVorbis
	CodecOpus
)

func (c Codec) String() string {
	switch c {
	case CodecVorbis:
		return "Vorbis"
	case CodecOpus:
		return "Opus"
	default:
		return "unknown"
	}
}

// WorldSampleRate is the required sample rate for fully-decoded world SFX (§6).
const WorldSampleRate = 44100

// MaxDecodedSetBytes caps the per-map RESIDENT (fully-decoded) audio set (§6).
// Streamed music is excluded — it is never fully resident.
const MaxDecodedSetBytes = 48 * 1024 * 1024

// Info is the recovered audio metadata.
type Info struct {
	Codec        Codec
	Channels     int
	SampleRate   int
	TotalSamples int64 // PCM sample frames, from the final page's granule position
}

// DecodedBytes is the fully-decoded 16-bit PCM size: frames × channels × 2.
func (i Info) DecodedBytes() int64 {
	return i.TotalSamples * int64(i.Channels) * 2
}

const oggCapture = "OggS"

// Parse reads the Ogg container metadata from an in-memory byte slice. Fails
// closed on a missing capture pattern, truncation, or an unrecognized codec
// header.
func Parse(data []byte) (Info, error) {
	return ParseReader(bytes.NewReader(data))
}

// ParseReader reads Ogg container metadata from r without retaining the full
// stream. It is intended for streamed music indexing: the runtime needs duration
// and layout information, not a preloaded compressed file or decoded PCM buffer.
func ParseReader(r io.Reader) (Info, error) {
	var firstPacket []byte
	var lastGranule int64
	pages := 0
	header := make([]byte, 27)
	for {
		_, err := io.ReadFull(r, header)
		if err == io.EOF {
			break
		}
		if err == io.ErrUnexpectedEOF {
			if pages == 0 {
				return Info{}, fmt.Errorf("not an Ogg stream (missing %q capture pattern)", oggCapture)
			}
			return Info{}, fmt.Errorf("truncated page header at page %d", pages)
		}
		if err != nil {
			return Info{}, err
		}
		if string(header[0:4]) != oggCapture {
			if pages == 0 {
				return Info{}, fmt.Errorf("not an Ogg stream (missing %q capture pattern)", oggCapture)
			}
			return Info{}, fmt.Errorf("bad page capture pattern at page %d", pages)
		}
		granule := int64(binary.LittleEndian.Uint64(header[6:14]))
		nseg := int(header[26])
		segs := make([]byte, nseg)
		if _, err := io.ReadFull(r, segs); err != nil {
			return Info{}, fmt.Errorf("truncated segment table at page %d", pages)
		}
		dataLen := 0
		for _, s := range segs {
			dataLen += int(s)
		}
		// The Vorbis/Opus identification header is required to sit alone on the BOS
		// page, so the first page's payload IS the identification packet.
		if firstPacket == nil && dataLen > 0 {
			firstPacket = make([]byte, dataLen)
			if _, err := io.ReadFull(r, firstPacket); err != nil {
				return Info{}, fmt.Errorf("truncated page data at page %d (need %d bytes)", pages, dataLen)
			}
		} else if dataLen > 0 {
			if _, err := io.CopyN(io.Discard, r, int64(dataLen)); err != nil {
				return Info{}, fmt.Errorf("truncated page data at page %d (need %d bytes)", pages, dataLen)
			}
		}
		lastGranule = granule
		pages++
	}
	if firstPacket == nil {
		return Info{}, fmt.Errorf("no audio packets found in Ogg stream")
	}
	info := Info{TotalSamples: lastGranule}
	if err := parseIDHeader(firstPacket, &info); err != nil {
		return Info{}, err
	}
	return info, nil
}

// parseIDHeader classifies the codec and pulls channels / sample rate.
func parseIDHeader(pkt []byte, info *Info) error {
	switch {
	case len(pkt) >= 16 && pkt[0] == 1 && string(pkt[1:7]) == "vorbis":
		info.Codec = CodecVorbis
		info.Channels = int(pkt[11])
		info.SampleRate = int(binary.LittleEndian.Uint32(pkt[12:16]))
	case len(pkt) >= 19 && string(pkt[0:8]) == "OpusHead":
		info.Codec = CodecOpus
		info.Channels = int(pkt[9])
		info.SampleRate = int(binary.LittleEndian.Uint32(pkt[12:16]))
	default:
		return fmt.Errorf("unrecognized codec identification header (not Vorbis or Opus)")
	}
	if info.Channels == 0 {
		return fmt.Errorf("%s header declares 0 channels", info.Codec)
	}
	return nil
}

// Category is the playback/layout class derived from an asset's path.
type Category uint8

const (
	CatWorldSFX Category = iota // mono 44.1 kHz, fully decoded (default)
	CatUI                       // mono or stereo, decoded at startup
	CatMusic                    // stereo, STREAMED (not resident)
	CatVoice                    // mono, decoded at map load
)

func (c Category) String() string {
	switch c {
	case CatUI:
		return "ui"
	case CatMusic:
		return "music"
	case CatVoice:
		return "voice"
	default:
		return "world-sfx"
	}
}

// CategoryOf classifies a slash-path (relative to the audio root) by directory.
func CategoryOf(rel string) Category {
	p := strings.ToLower(rel)
	switch {
	case strings.Contains(p, "music/"):
		return CatMusic
	case strings.Contains(p, "voice/"):
		return CatVoice
	case strings.Contains(p, "ui/"):
		return CatUI
	default:
		return CatWorldSFX
	}
}

// Finding is one layout-rule violation.
type Finding struct {
	Rule string
	Msg  string
}

// CheckLayout applies the codec + channel-layout rules for an asset of category
// cat, returning findings and whether its decoded bytes count toward the resident
// per-map budget (music streams, so it does not).
func CheckLayout(info Info, cat Category) (findings []Finding, resident bool) {
	add := func(rule, msg string) { findings = append(findings, Finding{rule, msg}) }

	// Codec gate (R-AUD-1.2): Vorbis-in-Ogg only — codec check, not extension check,
	// so an Opus stream inside a .ogg container is still rejected.
	if info.Codec != CodecVorbis {
		add("AUD-CODEC", fmt.Sprintf("%s codec in .ogg container — only Ogg Vorbis is supported (R-AUD-1.2)", info.Codec))
		return findings, cat != CatMusic // still report budget membership truthfully
	}

	switch cat {
	case CatWorldSFX:
		resident = true
		if info.Channels != 1 {
			add("AUD-CHAN", fmt.Sprintf("world SFX must be mono, got %d channels — stereo in a world-SFX dir is a build error (R-AUD-1.6)", info.Channels))
		}
		if info.SampleRate != WorldSampleRate {
			add("AUD-RATE", fmt.Sprintf("world SFX must be %d Hz, got %d Hz (R-AUD-1.6)", WorldSampleRate, info.SampleRate))
		}
	case CatVoice:
		resident = true
		if info.Channels != 1 {
			add("AUD-CHAN", fmt.Sprintf("voice line must be mono, got %d channels (R-AUD-1.6)", info.Channels))
		}
	case CatUI:
		resident = true
		if info.Channels > 2 {
			add("AUD-CHAN", fmt.Sprintf("UI SFX must be mono or stereo, got %d channels (R-AUD-1.6)", info.Channels))
		}
	case CatMusic:
		resident = false // streamed, never fully resident
		if info.Channels != 2 {
			add("AUD-CHAN", fmt.Sprintf("music must be stereo, got %d channels (R-AUD-1.6)", info.Channels))
		}
	}
	return findings, resident
}
