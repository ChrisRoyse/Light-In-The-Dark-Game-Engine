package audio

import (
	"bytes"
	"encoding/binary"
	"strings"
	"testing"
	"testing/fstest"
)

func loaderOggPage(headerType byte, granule int64, seq uint32, packet []byte) []byte {
	var segs []byte
	n := len(packet)
	for n >= 255 {
		segs = append(segs, 255)
		n -= 255
	}
	segs = append(segs, byte(n))
	var b bytes.Buffer
	b.WriteString("OggS")
	b.WriteByte(0)
	b.WriteByte(headerType)
	_ = binary.Write(&b, binary.LittleEndian, granule)
	_ = binary.Write(&b, binary.LittleEndian, uint32(0xA0D10))
	_ = binary.Write(&b, binary.LittleEndian, seq)
	_ = binary.Write(&b, binary.LittleEndian, uint32(0))
	b.WriteByte(byte(len(segs)))
	b.Write(segs)
	b.Write(packet)
	return b.Bytes()
}

func loaderVorbisID(channels byte, rate uint32) []byte {
	var b bytes.Buffer
	b.WriteByte(1)
	b.WriteString("vorbis")
	_ = binary.Write(&b, binary.LittleEndian, uint32(0))
	b.WriteByte(channels)
	_ = binary.Write(&b, binary.LittleEndian, rate)
	_ = binary.Write(&b, binary.LittleEndian, int32(0))
	_ = binary.Write(&b, binary.LittleEndian, int32(0))
	_ = binary.Write(&b, binary.LittleEndian, int32(0))
	b.WriteByte(0xB8)
	b.WriteByte(1)
	return b.Bytes()
}

func loaderOpusHead(channels byte) []byte {
	var b bytes.Buffer
	b.WriteString("OpusHead")
	b.WriteByte(1)
	b.WriteByte(channels)
	_ = binary.Write(&b, binary.LittleEndian, uint16(0))
	_ = binary.Write(&b, binary.LittleEndian, uint32(48000))
	_ = binary.Write(&b, binary.LittleEndian, uint16(0))
	b.WriteByte(0)
	return b.Bytes()
}

func loaderOgg(id []byte, frames int64) []byte {
	return append(loaderOggPage(0x02, 0, 0, id), loaderOggPage(0x04, frames, 1, nil)...)
}

func vorbisOgg(channels byte, rate uint32, frames int64) []byte {
	return loaderOgg(loaderVorbisID(channels, rate), frames)
}

func opusOgg(channels byte, frames int64) []byte {
	return loaderOgg(loaderOpusHead(channels), frames)
}

func assetByPath(d LoadDump, p string) (LoadedAsset, bool) {
	for _, a := range d.Assets {
		if a.Path == p {
			return a, true
		}
	}
	return LoadedAsset{}, false
}

func TestLoadAssetsHappyPathFSV(t *testing.T) {
	fsys := fstest.MapFS{
		"audio/sfx/sword.ogg":   {Data: vorbisOgg(1, 44100, 44100)},
		"audio/ui/click.ogg":    {Data: vorbisOgg(2, 44100, 22050)},
		"audio/music/theme.ogg": {Data: vorbisOgg(2, 44100, 44100*60)},
	}
	d, err := LoadAssets(fsys, "audio")
	if err != nil {
		t.Fatalf("happy path rejected: %v\n%+v", err, d)
	}
	if !d.OK || d.AssetCount != 3 {
		t.Fatalf("dump header wrong: %+v", d)
	}
	// SoT read-back: resident = sword (44100*1*2) + click (22050*2*2);
	// music is streamed, so its 60s decoded size must not enter resident bytes.
	wantResident := int64(44100*1*2 + 22050*2*2)
	if d.ResidentDecodedBytes != wantResident {
		t.Fatalf("resident bytes want %d, got %d in %+v", wantResident, d.ResidentDecodedBytes, d)
	}
	music, ok := assetByPath(d, "music/theme.ogg")
	if !ok {
		t.Fatalf("music asset missing from dump: %+v", d.Assets)
	}
	if !music.Streamed || music.Resident || music.StreamChunkBytes != StreamChunkBytes || music.StreamRingChunks != StreamRingChunks {
		t.Fatalf("music must be streamed with bounded ring, got %+v", music)
	}
	if music.DecodedBytes <= int64(music.StreamBufferBytes) {
		t.Fatalf("60s music decoded bytes should be much larger than stream buffer: %+v", music)
	}
	sword, ok := assetByPath(d, "sfx/sword.ogg")
	if !ok || !sword.Resident || sword.ResidentBufferSize != 44100*2 || sword.Streamed {
		t.Fatalf("world SFX resident buffer wrong: %+v ok=%v", sword, ok)
	}
	t.Logf("FSV #228 loader happy: resident=%d streamBuffer=%d musicDecoded=%d", d.ResidentDecodedBytes, d.StreamBufferBytes, music.DecodedBytes)
}

func TestLoadAssetsRejectsEdgesFSV(t *testing.T) {
	cases := []struct {
		name string
		fsys fstest.MapFS
		want string
	}{
		{
			name: "stereo-world",
			fsys: fstest.MapFS{"sfx/hit.ogg": {Data: vorbisOgg(2, 44100, 100)}},
			want: "AUD-CHAN",
		},
		{
			name: "opus-codec",
			fsys: fstest.MapFS{"music/theme.ogg": {Data: opusOgg(2, 100)}},
			want: "AUD-CODEC",
		},
		{
			name: "bad-container",
			fsys: fstest.MapFS{"sfx/bad.ogg": {Data: []byte("RIFFxxxxWAVE")}},
			want: "AUD-PARSE",
		},
		{
			name: "non-ogg-audio",
			fsys: fstest.MapFS{"sfx/bad.wav": {Data: []byte("x")}},
			want: "AUD-FMT",
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			d, err := LoadAssets(c.fsys, ".")
			if err == nil || d.OK {
				t.Fatalf("%s accepted unexpectedly: err=%v dump=%+v", c.name, err, d)
			}
			if !strings.Contains(strings.Join(d.Errors, "\n"), c.want) {
				t.Fatalf("%s errors %v do not contain %s", c.name, d.Errors, c.want)
			}
			t.Logf("FSV #228 loader edge %s: errors=%v", c.name, d.Errors)
		})
	}
}

func TestLoadAssetsBudgetFSV(t *testing.T) {
	const mb = 1024 * 1024
	overFrames := int64(49*mb/2 + 1)
	underFrames := int64(47 * mb / 2)

	over, err := LoadAssets(fstest.MapFS{"sfx/big.ogg": {Data: vorbisOgg(1, 44100, overFrames)}}, ".")
	if err == nil || over.OK {
		t.Fatalf("49 MB resident set accepted: err=%v dump=%+v", err, over)
	}
	if !strings.Contains(strings.Join(over.Errors, "\n"), "AUD-BUDGET") {
		t.Fatalf("49 MB resident set should yield AUD-BUDGET, got %v", over.Errors)
	}
	under, err := LoadAssets(fstest.MapFS{"sfx/ok.ogg": {Data: vorbisOgg(1, 44100, underFrames)}}, ".")
	if err != nil || !under.OK {
		t.Fatalf("47 MB resident set rejected: err=%v dump=%+v", err, under)
	}
	t.Logf("FSV #228 loader budget: over=%d rejected; under=%d accepted", over.ResidentDecodedBytes, under.ResidentDecodedBytes)
}
