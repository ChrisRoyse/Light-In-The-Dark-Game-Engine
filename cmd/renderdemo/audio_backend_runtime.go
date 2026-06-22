//go:build !openal

package main

import (
	"errors"

	litaudio "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
)

var errOpenALNotCompiled = errors.New("audio: OpenAL backend not compiled; rebuild with -tags openal")

func openAudioBackendForDemo() (litaudio.Backend, error) {
	return nil, errOpenALNotCompiled
}
