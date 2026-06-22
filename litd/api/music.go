package litd

// Music + channel-mix surface (#244, sound-and-music.md). Like the Sound noun,
// these are presentation-only and sim-inert (R-AUD-1): headless they validate
// + clamp and no-op; the render driver hears them through the Game.OnAudio
// sink. Music lives on Game (there is one music stream) rather than a noun.

// PlayMusic starts a music cue (looping by default at the render layer). An
// empty cue is a no-op. JASS: PlayMusic, PlayMusicExBJ, PlayThematicMusic
// collapse here (loop/offset/fade are render-layer concerns).
// JASS: PlayMusic, PlayMusicBJ, PlayMusicEx, PlayMusicExBJ, PlayThematicMusic, PlayThematicMusicBJ, PlayThematicMusicEx, PlayThematicMusicExBJ, ResumeMusic, ResumeMusicBJ
func (g *Game) PlayMusic(cue string) {
	if g == nil || cue == "" {
		return
	}
	g.emitAudio(AudioEvent{Kind: AudioPlayMusic, Cue: CueID(cue), Volume: 1, Channel: ChannelMusic})
}

// StopMusic stops the current music stream. JASS: StopMusic,
// EndThematicMusic collapse here.
// JASS: EndThematicMusic, EndThematicMusicBJ, StopMusic, StopMusicBJ
func (g *Game) StopMusic() {
	if g == nil {
		return
	}
	g.emitAudio(AudioEvent{Kind: AudioStopMusic})
}

// SetMusicVolume sets the music stream volume on a 0..1 scale (clamped). JASS:
// SetMusicVolume, SetMusicVolumeBJ (0..100 percent) collapse onto the float.
// JASS: SetMusicVolume, SetMusicVolumeBJ, SetThematicMusicVolume, SetThematicMusicVolumeBJ
func (g *Game) SetMusicVolume(v float64) {
	if g == nil {
		return
	}
	g.emitAudio(AudioEvent{Kind: AudioSetMusicVolume, Volume: clamp01(v), Channel: ChannelMusic})
}

// SetChannelVolume sets a mix channel's master volume on a 0..1 scale
// (clamped). JASS: SetSoundChannelVolume and the per-group volume BJs collapse
// onto this enum-keyed setter (dedup §6).
// JASS: VolumeGroupSetVolume, VolumeGroupSetVolumeBJ, VolumeGroupSetVolumeForPlayerBJ
func (g *Game) SetChannelVolume(ch SoundChannel, v float64) {
	if g == nil {
		return
	}
	g.emitAudio(AudioEvent{Kind: AudioSetChannelVolume, Channel: ch, Volume: clamp01(v)})
}
