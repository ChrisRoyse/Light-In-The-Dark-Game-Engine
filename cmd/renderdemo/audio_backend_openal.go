//go:build openal

package main

import litaudio "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"

func openAudioBackendForDemo() (litaudio.Backend, error) {
	return litaudio.OpenDevice()
}
