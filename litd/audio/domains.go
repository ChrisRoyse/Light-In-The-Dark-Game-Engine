package audio

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// Audio playback domains (#231, audio.md §4 R-AUD-1.3, §8). There are EXACTLY two:
//
//   - World — 3D positional, inverse-distance attenuated, stereo-panned from the
//     fixed camera, distance-culled, and voice-budgeted. Off-screen fights are
//     audible but quiet; sounds far enough away are culled entirely.
//   - UI    — 2D flat, full volume, centered, never attenuated or culled. The
//     under-attack stinger, acknowledgement voice lines, and minimap pings are UI:
//     the player MUST hear them regardless of where the camera is (the WC3 rule).
//
// The domain is a property of the ASSET (its data-table entry), not of whether a
// play call carried a position — a UI sound played "at" a unit is still flat. The
// per-asset domain/priority data table + its assetcheck gate are the authoritative
// source when installed. The channel mapping below remains the fallback for
// unclassified rollout fixtures and direct tests.
//
// This file is the pure resolution model: given a domain, a source/listener
// geometry, and the volume inputs, it yields the final gain/pan (or a cull). It is
// device-independent and allocation-free, so it is the single source of the numbers
// the null and OpenAL backends both play.

// Domain is one of the two playback domains.
type Domain uint8

const (
	DomainWorld Domain = iota // 3D positional, attenuated, budgeted, cullable
	DomainUI                  // 2D flat, full volume, exempt from attenuation/cull
)

// VolumeGroup is one of the three independently-settable master groups (§8).
type VolumeGroup uint8

const (
	GroupWorld VolumeGroup = iota
	GroupUI
	GroupMusic
)

const numGroups = 3

// Distance model (inverse-distance clamped, OpenAL AL_INVERSE_DISTANCE_CLAMPED).
// Expressed in world units; the constants are sized to the camera so that one
// reference distance ≈ a screen-height and the cull radius ≈ 1.5 screens past the
// viewport edge — off-screen combat stays faintly audible, distant combat does not.
const (
	// ReferenceDistance is the range within which a world sound plays at full gain;
	// beyond it gain rolls off as ReferenceDistance/distance.
	ReferenceDistance = 400.0
	// MaxAudibleDistance is the hard cull radius: a world sound past it is silent
	// (Culled), never merely quiet. Kept equal to the mixer's FalloffRadius so the
	// two agree on "audible".
	MaxAudibleDistance = FalloffRadius
)

// Resolved is the output of domain resolution: the final gain/pan, or a cull.
type Resolved struct {
	Gain   float64
	Pan    float64
	Culled bool // world sound beyond MaxAudibleDistance — do not play
}

// ResolveWorld applies the 3D world model: inverse-distance-clamped attenuation
// plus stereo pan from the listener. vol is the [0,1] request volume; groupVol is
// the World master group. A source beyond MaxAudibleDistance is culled.
func ResolveWorld(src, listener Vec3, vol, groupVol float64) Resolved {
	dist := src.sub(listener).length()
	if dist > MaxAudibleDistance {
		return Resolved{Culled: true}
	}
	atten := 1.0
	if dist > ReferenceDistance {
		atten = ReferenceDistance / dist // monotonically decreasing, in (0,1]
	}
	return Resolved{
		Gain: clamp(vol*groupVol*atten, 0, 1),
		Pan:  clamp((src.X-listener.X)/PanWidth, -1, 1),
	}
}

// ResolveFlat applies the flat model (UI sounds, and non-positional world sounds):
// full requested volume scaled only by the group, centered, never culled.
func ResolveFlat(vol, groupVol float64) Resolved {
	return Resolved{Gain: clamp(vol*groupVol, 0, 1), Pan: 0}
}

// DomainOf classifies a mixer channel into a playback domain for fallback paths
// without a per-asset table row. The UI channel is the UI domain; every other
// channel is World.
func DomainOf(ch api.SoundChannel) Domain {
	if ch == api.ChannelUI {
		return DomainUI
	}
	return DomainWorld
}

// GroupOf maps a mixer channel to its master volume group: UI→UI, Music/Ambient→
// Music, everything else→World.
func GroupOf(ch api.SoundChannel) VolumeGroup {
	switch ch {
	case api.ChannelUI:
		return GroupUI
	case api.ChannelMusic, api.ChannelAmbient:
		return GroupMusic
	default:
		return GroupWorld
	}
}

// GroupForDomain maps a per-asset playback domain (the authoritative #428
// classification) to its master volume group: UI→UI, World→World. This is the
// data-driven counterpart to GroupOf's channel inference.
func GroupForDomain(d Domain) VolumeGroup {
	if d == DomainUI {
		return GroupUI
	}
	return GroupWorld
}
