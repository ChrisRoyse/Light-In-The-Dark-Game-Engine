use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::*;

#[test]
fn contained_path_resolves_from_physical_bytes() {
    let fixture = Fixture::new("contained");
    let vault = fixture.root.join("vaults").join("01TESTVAULT");
    fs::create_dir_all(&vault).unwrap();

    let resolved = validate_existing_path_under_root(&fixture.root, &vault, "test vault").unwrap();

    assert_eq!(path_key(&resolved), path_key(&vault));
}

#[test]
fn existing_outside_path_fails_closed() {
    let fixture = Fixture::new("outside");
    let outside = fixture.base.join("outside");
    fs::create_dir_all(&outside).unwrap();

    let error =
        validate_existing_path_under_root(&fixture.root, &outside, "outside vault").unwrap_err();

    assert_eq!(error.code, CALYX_PATH_OUTSIDE_ROOT);
}

#[test]
fn missing_root_fails_closed() {
    let fixture = Fixture::new("missing-root");
    let missing = fixture.base.join("missing");

    let error = canonical_root(&missing).unwrap_err();

    assert_eq!(error.code, CALYX_CANONICAL_ROOT_MISSING);
}

#[cfg(windows)]
#[test]
fn directory_junction_escape_fails_closed() {
    use std::process::Command;

    let fixture = Fixture::new("link-escape");
    let outside = fixture.base.join("outside");
    let link = fixture.root.join("linked-vault");
    fs::create_dir_all(&outside).unwrap();
    let output = Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "create junction failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let error =
        validate_existing_path_under_root(&fixture.root, &link, "linked vault").unwrap_err();

    assert_eq!(error.code, CALYX_PATH_ESCAPE);
}

#[test]
fn named_vault_requires_structured_index_path_identity() {
    let fixture = Fixture::new("index");
    let vault_id = "01TESTVAULT";
    let vault = fixture.root.join("vaults").join(vault_id);
    fs::create_dir_all(&vault).unwrap();
    fs::write(
        fixture.root.join("vaults").join("index.json"),
        format!(
            r#"{{"vaults":[{{"name":"{ENGINE_CONTENT_PANEL_VAULT_NAME}","vault_id":"{vault_id}","path":"vaults/{vault_id}"}}]}}"#
        ),
    )
    .unwrap();

    let resolved =
        resolve_named_vault_under_root(&fixture.root, ENGINE_CONTENT_PANEL_VAULT_NAME).unwrap();

    assert_eq!(path_key(&resolved), path_key(&vault));
}

struct Fixture {
    base: PathBuf,
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let base = Path::new(GAMEEDITOR_CALYX_ROOT)
            .join("tmp")
            .join("issue1503-calyx-path-tests")
            .join(format!(
                "{name}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let root = base.join("root");
        fs::create_dir_all(&root).unwrap();
        Self { base, root }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).ok();
        if let Some(scratch_root) = self.base.parent() {
            fs::remove_dir(scratch_root).ok();
        }
    }
}
