//go:build !openal

package main

import (
	"strings"
	"testing"
)

func TestBuildAudioInitDumpAutoNoOpenALFallbackFSV(t *testing.T) {
	dump, err := buildAudioInitDump("auto")
	if err != nil {
		t.Fatalf("auto mode should fall back to null when OpenAL is not compiled: %v", err)
	}
	if !dump.OK {
		t.Fatalf("auto fallback dump failed: %+v", dump.Errors)
	}
	if dump.Backend != "null" || dump.BackendSources != 0 {
		t.Fatalf("auto fallback should select null backend with zero sources: %+v", dump)
	}
	if !strings.Contains(dump.DeviceError, "OpenAL backend not compiled") {
		t.Fatalf("auto fallback must capture the device/open error, got %q", dump.DeviceError)
	}
	t.Logf("FSV #227 renderdemo auto fallback BEFORE openal=not-compiled AFTER backend=%s sources=%d deviceError=%q ok=%v",
		dump.Backend, dump.BackendSources, dump.DeviceError, dump.OK)
}
