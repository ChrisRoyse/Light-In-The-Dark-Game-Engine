use std::io::Cursor;

use lewton::inside_ogg::OggStreamReader;
use litd_engine::EngineError;

use super::{SoundAudioCodec, invalid_sound_editor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AudioMetadata {
    pub codec: SoundAudioCodec,
    pub length_ms: u64,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub bits_per_sample: Option<u16>,
}

pub(super) fn parse_audio_metadata(
    extension: &str,
    bytes: &[u8],
) -> Result<AudioMetadata, EngineError> {
    match extension {
        "wav" => parse_wav_metadata(bytes),
        "ogg" => parse_ogg_vorbis_metadata(bytes),
        other => invalid_sound_editor(
            "import.extension",
            format!("unsupported sound editor import extension {other:?}"),
        ),
    }
}

fn parse_wav_metadata(bytes: &[u8]) -> Result<AudioMetadata, EngineError> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes))
        .map_err(|error| audio_decode_error("wav.decode.header", error))?;
    let spec = reader.spec();
    if spec.sample_format != hound::SampleFormat::Int {
        return invalid_sound_editor("wav.fmt.format", "only integer PCM WAV is supported");
    }
    if spec.channels == 0 || spec.sample_rate == 0 {
        return invalid_sound_editor("wav.fmt.zero", "WAV format values must be non-zero");
    }
    if spec.bits_per_sample == 0 || spec.bits_per_sample % 8 != 0 {
        return invalid_sound_editor("wav.fmt.bits", "bits per sample must be byte-aligned");
    }
    let decoded_frames = u64::from(reader.duration());
    if decoded_frames == 0 {
        return invalid_sound_editor("wav.length", "decoded length must be non-zero");
    }
    for sample in reader.samples::<i32>() {
        sample.map_err(|error| audio_decode_error("wav.decode.samples", error))?;
    }
    let length_ms = ((u128::from(decoded_frames) * 1000) / u128::from(spec.sample_rate)) as u64;
    if length_ms == 0 {
        return invalid_sound_editor("wav.length", "decoded length must be non-zero");
    }
    Ok(AudioMetadata {
        codec: SoundAudioCodec::PcmWav,
        length_ms,
        sample_rate_hz: spec.sample_rate,
        channels: spec.channels,
        bits_per_sample: Some(spec.bits_per_sample),
    })
}

fn parse_ogg_vorbis_metadata(bytes: &[u8]) -> Result<AudioMetadata, EngineError> {
    let mut reader = OggStreamReader::new(Cursor::new(bytes))
        .map_err(|error| audio_decode_error("ogg.decode.headers", error))?;
    let channels = u16::from(reader.ident_hdr.audio_channels);
    let sample_rate_hz = reader.ident_hdr.audio_sample_rate;
    if channels == 0 || sample_rate_hz == 0 {
        return invalid_sound_editor(
            "ogg.identification",
            "channels and sample rate must be non-zero",
        );
    }
    let mut decoded_frames = 0u64;
    while let Some(packet) = reader
        .read_dec_packet_itl()
        .map_err(|error| audio_decode_error("ogg.decode.packet", error))?
    {
        if packet.len() % usize::from(channels) != 0 {
            return invalid_sound_editor(
                "ogg.decode.packet",
                "decoded packet is not aligned to the channel count",
            );
        }
        decoded_frames += (packet.len() / usize::from(channels)) as u64;
    }
    if decoded_frames == 0 {
        return invalid_sound_editor("ogg.length", "decoded length must be non-zero");
    }
    let length_ms = ((u128::from(decoded_frames) * 1000) / u128::from(sample_rate_hz)) as u64;
    if length_ms == 0 {
        return invalid_sound_editor("ogg.length", "decoded length must be non-zero");
    }
    Ok(AudioMetadata {
        codec: SoundAudioCodec::OggVorbis,
        length_ms,
        sample_rate_hz,
        channels,
        bits_per_sample: None,
    })
}

fn audio_decode_error(field: &str, error: impl std::fmt::Display) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.sound_editor.{field}"),
        reason: error.to_string(),
    }
}
