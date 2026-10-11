use std::{fs, path::PathBuf};

use super::{SoundCategory, import_sound_file};

#[test]
fn malformed_ogg_page_with_fake_vorbis_header_fails_decoder_validation() {
    let (_temp_root, path) = temp_audio_path("fake_ident_only", "ogg");
    fs::write(&path, fake_ident_only_ogg_page()).unwrap();

    let error = import_sound_file(&path, SoundCategory::Imported, "snd_fake_ogg")
        .unwrap_err()
        .to_string();

    assert!(error.contains("editor.sound_editor.ogg.decode.headers"));
}

fn fake_ident_only_ogg_page() -> Vec<u8> {
    let mut body = vec![0u8; 30];
    body[0] = 1;
    body[1..7].copy_from_slice(b"vorbis");
    body[11] = 2;
    body[12..16].copy_from_slice(&44_100u32.to_le_bytes());

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"OggS");
    bytes.push(0);
    bytes.push(2);
    bytes.extend_from_slice(&44_100u64.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.push(1);
    bytes.push(body.len() as u8);
    bytes.extend_from_slice(&body);
    bytes
}

// #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
fn temp_audio_path(label: &str, extension: &str) -> (litd_engine::test_temp::TestTempDir, PathBuf) {
    let temp = litd_engine::test_temp::TestTempDir::new(&format!("issue1103-audio-{label}"));
    fs::create_dir_all(temp.path()).unwrap();
    let path = temp
        .path()
        .join(format!("litd_issue1103_audio_import.{extension}"));
    (temp, path)
}
