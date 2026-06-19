package oggmeta

// #228 FSV. SoT = the Info parsed from a synthetic Ogg container we byte-construct
// (known codec/channels/rate/granule) + the findings from CheckLayout. X+X: a
// 44.1 kHz mono 1-second Vorbis stream → 44100 frames → 88,200 decoded bytes.

import (
	"bytes"
	"encoding/binary"
	"strings"
	"testing"
)

// oggPage frames one packet into a single Ogg page (CRC left zero — not validated).
func oggPage(headerType byte, granule int64, seq uint32, packet []byte) []byte {
	var segs []byte
	n := len(packet)
	for n >= 255 {
		segs = append(segs, 255)
		n -= 255
	}
	segs = append(segs, byte(n))
	var b bytes.Buffer
	b.WriteString("OggS")
	b.WriteByte(0) // stream structure version
	b.WriteByte(headerType)
	binary.Write(&b, binary.LittleEndian, granule)
	binary.Write(&b, binary.LittleEndian, uint32(0xCAFE)) // serial
	binary.Write(&b, binary.LittleEndian, seq)
	binary.Write(&b, binary.LittleEndian, uint32(0)) // CRC
	b.WriteByte(byte(len(segs)))
	b.Write(segs)
	b.Write(packet)
	return b.Bytes()
}

// vorbisID builds a Vorbis identification header packet.
func vorbisID(channels byte, rate uint32) []byte {
	var b bytes.Buffer
	b.WriteByte(1) // packet type = identification
	b.WriteString("vorbis")
	binary.Write(&b, binary.LittleEndian, uint32(0)) // vorbis version
	b.WriteByte(channels)
	binary.Write(&b, binary.LittleEndian, rate)
	binary.Write(&b, binary.LittleEndian, int32(0)) // bitrate max
	binary.Write(&b, binary.LittleEndian, int32(0)) // bitrate nominal
	binary.Write(&b, binary.LittleEndian, int32(0)) // bitrate min
	b.WriteByte(0xB8)                                // blocksizes
	b.WriteByte(1)                                   // framing bit
	return b.Bytes()
}

// opusHead builds an OpusHead identification header packet.
func opusHead(channels byte, rate uint32) []byte {
	var b bytes.Buffer
	b.WriteString("OpusHead")
	b.WriteByte(1)        // version
	b.WriteByte(channels) // channel count
	binary.Write(&b, binary.LittleEndian, uint16(0)) // pre-skip
	binary.Write(&b, binary.LittleEndian, rate)      // input sample rate
	binary.Write(&b, binary.LittleEndian, uint16(0)) // output gain
	b.WriteByte(0)                                   // channel mapping family
	return b.Bytes()
}

// buildOgg = a BOS page carrying the id header + an EOS page carrying the granule.
func buildOgg(idPacket []byte, totalSamples int64) []byte {
	bos := oggPage(0x02, 0, 0, idPacket)        // BOS
	eos := oggPage(0x04, totalSamples, 1, []byte{}) // EOS, empty packet
	return append(bos, eos...)
}

func TestParseVorbisInfoFSV(t *testing.T) {
	data := buildOgg(vorbisID(1, 44100), 44100) // mono, 44.1 kHz, 1 second
	info, err := Parse(data)
	if err != nil {
		t.Fatalf("parse valid Vorbis: %v", err)
	}
	if info.Codec != CodecVorbis || info.Channels != 1 || info.SampleRate != 44100 {
		t.Fatalf("want Vorbis/1ch/44100, got %s/%dch/%dHz", info.Codec, info.Channels, info.SampleRate)
	}
	if info.TotalSamples != 44100 {
		t.Fatalf("total samples: want 44100 (final granule), got %d", info.TotalSamples)
	}
	if db := info.DecodedBytes(); db != 44100*1*2 {
		t.Fatalf("decoded bytes: want %d (44100×1×2), got %d", 44100*2, db)
	}
	t.Logf("FSV #228 parse: Vorbis 1ch 44100Hz 44100 frames → %d decoded bytes", info.DecodedBytes())
}

func TestCodecRejectOpusFSV(t *testing.T) {
	// Opus inside a .ogg container — must be rejected by CODEC check, not extension.
	data := buildOgg(opusHead(2, 48000), 48000)
	info, err := Parse(data)
	if err != nil {
		t.Fatalf("parse opus container: %v", err)
	}
	if info.Codec != CodecOpus {
		t.Fatalf("want Opus codec detected, got %s", info.Codec)
	}
	fs, _ := CheckLayout(info, CatMusic)
	if len(fs) == 0 || fs[0].Rule != "AUD-CODEC" {
		t.Fatalf("Opus-in-ogg must yield AUD-CODEC, got %+v", fs)
	}
	t.Logf("FSV #228 codec: Opus-in-ogg rejected → %s: %s", fs[0].Rule, fs[0].Msg)
}

func TestStereoWorldSFXRejectedFSV(t *testing.T) {
	info, _ := Parse(buildOgg(vorbisID(2, 44100), 1000)) // stereo
	fs, resident := CheckLayout(info, CatWorldSFX)
	if !resident {
		t.Fatal("world SFX must count as resident")
	}
	found := false
	for _, f := range fs {
		if f.Rule == "AUD-CHAN" {
			found = true
			t.Logf("FSV #228 layout: stereo world SFX rejected → %s: %s", f.Rule, f.Msg)
		}
	}
	if !found {
		t.Fatalf("stereo world SFX must yield AUD-CHAN, got %+v", fs)
	}
}

func TestValidLayoutsCleanFSV(t *testing.T) {
	cases := []struct {
		name string
		info Info
		cat  Category
		res  bool
	}{
		{"mono world sfx", Info{CodecVorbis, 1, 44100, 100}, CatWorldSFX, true},
		{"stereo music", Info{CodecVorbis, 2, 44100, 100}, CatMusic, false},
		{"mono voice", Info{CodecVorbis, 1, 44100, 100}, CatVoice, true},
		{"stereo ui", Info{CodecVorbis, 2, 44100, 100}, CatUI, true},
	}
	for _, c := range cases {
		fs, resident := CheckLayout(c.info, c.cat)
		if len(fs) != 0 {
			t.Fatalf("%s: want clean, got %+v", c.name, fs)
		}
		if resident != c.res {
			t.Fatalf("%s: resident want %v got %v", c.name, c.res, resident)
		}
		t.Logf("FSV #228 clean: %-16s cat=%s resident=%v", c.name, c.cat, resident)
	}
}

func TestMusicMustBeStereoFSV(t *testing.T) {
	info, _ := Parse(buildOgg(vorbisID(1, 44100), 100)) // mono music
	fs, resident := CheckLayout(info, CatMusic)
	if resident {
		t.Fatal("music must be streamed (non-resident)")
	}
	if len(fs) == 0 || fs[0].Rule != "AUD-CHAN" {
		t.Fatalf("mono music must yield AUD-CHAN, got %+v", fs)
	}
	t.Logf("FSV #228 music: mono music rejected → %s", fs[0].Msg)
}

func TestParseRejectsMalformedFSV(t *testing.T) {
	cases := []struct {
		name string
		data []byte
	}{
		{"not ogg", []byte("RIFFxxxxWAVE")},
		{"too short", []byte("Og")},
		{"truncated segment table", []byte("OggS\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00\xfe\xca\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x05")},
	}
	for _, c := range cases {
		if _, err := Parse(c.data); err == nil {
			t.Fatalf("%s: must fail closed, got nil error", c.name)
		} else {
			t.Logf("FSV #228 malformed: %-24s → rejected: %v", c.name, err)
		}
	}
}

func TestCategoryOfFSV(t *testing.T) {
	cases := map[string]Category{
		"sfx/world/sword.ogg": CatWorldSFX,
		"units/footman.ogg":   CatWorldSFX,
		"music/theme.ogg":     CatMusic,
		"voice/ack-1.ogg":     CatVoice,
		"ui/click.ogg":        CatUI,
	}
	for path, want := range cases {
		if got := CategoryOf(path); got != want {
			t.Fatalf("CategoryOf(%q): want %s, got %s", path, want, got)
		}
	}
	if !strings.Contains(CatMusic.String(), "music") {
		t.Fatal("category String() broken")
	}
	t.Logf("FSV #228 category: paths classified world/music/voice/ui correctly")
}
