use std::{fs, path::Path, time::Instant};

use litd_editor::{
    history::EditorObject,
    object_manager::{
        ObjectManagerIndex, ObjectManagerProject, build_object_manager_index, find_references,
        issue736_sample_project, rename_object_and_refs, search_entries,
    },
};
use litd_engine::math::Transform;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Issue736Fsv {
    schema: String,
    issue: u32,
    source_of_truth: Value,
    happy_path: Value,
    edges: Value,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_OBJECT_MANAGER_ISSUE736_FSV_ROOT",
        "issue736_object_manager",
        "manual",
    )?;
    reset_root(&root)?;
    let project_path = root.join("object-manager-project.json");
    let index_path = root.join("object-manager-index.json");
    let report_path = root.join("issue736-object-manager-fsv-readback.json");
    let project = issue736_sample_project()?;
    let index = build_object_manager_index(&project)?;
    assert_happy_path(&index)?;
    write_json(&project_path, &project)?;
    write_json(&index_path, &index)?;
    let project_readback: ObjectManagerProject = read_json(&project_path)?;
    let index_readback: ObjectManagerIndex = read_json(&index_path)?;
    if project_readback != project || index_readback != index {
        return Err("object manager project/index readback mismatch".into());
    }
    let guard_refs = find_references(&index, "unit.issue736.guard");
    let report = Issue736Fsv {
        schema: "litd.editor.issue736.object_manager_fsv.v1".to_string(),
        issue: 736,
        source_of_truth: json!({
            "project": project_path,
            "index": index_path,
            "report": report_path
        }),
        happy_path: json!({
            "counts": index.counts,
            "entry_count": index.entries.len(),
            "reference_count": index.references.len(),
            "fingerprint": index.fingerprint,
            "guard_refs": guard_refs,
            "search_guard": search_entries(&index, "guard", 8),
            "project_sha256": sha256_file(&project_path)?,
            "index_sha256": sha256_file(&index_path)?
        }),
        edges: json!({
            "orphan_flagged": edge_orphan(&project)?,
            "rename_updates_refs": edge_rename(&project)?,
            "large_project_search_responsive": edge_large_search(&project)?
        }),
    };
    write_json(&report_path, &report)?;
    let report_readback: Issue736Fsv = read_json(&report_path)?;
    if report_readback != report {
        return Err(format!("report readback mismatch at {}", report_path.display()).into());
    }
    let report_artifact = fsv_run.record_json_file(&report_path)?;
    println!(
        "ISSUE736_OBJECT_MANAGER_FSV report={} report_sha256={} project_sha256={} index_sha256={}",
        report_artifact.path.display(),
        report_artifact.sha256,
        sha256_file(&project_path)?,
        sha256_file(&index_path)?,
    );
    Ok(())
}

fn assert_happy_path(index: &ObjectManagerIndex) -> Result<(), Box<dyn std::error::Error>> {
    for (key, expected) in [
        ("objects", 4),
        ("unit", 2),
        ("ability", 2),
        ("regions", 1),
        ("triggers", 1),
        ("assets", 2),
        ("total", 8),
    ] {
        if index.counts.get(key).copied() != Some(expected) {
            return Err(format!("count {key} did not equal {expected}").into());
        }
    }
    let guard_refs = find_references(index, "unit.issue736.guard");
    if !guard_refs
        .iter()
        .any(|reference| reference.source_id == "trg_issue736_guard_death")
    {
        return Err("guard unit did not list trigger reference".into());
    }
    Ok(())
}

fn edge_orphan(project: &ObjectManagerProject) -> Result<Value, Box<dyn std::error::Error>> {
    let mut edge = project.clone();
    edge.document.upsert_object(EditorObject::new(
        "unit.issue736.orphan",
        "Unit",
        Some("unit.issue736.missing_parent".to_string()),
        Transform::IDENTITY,
    ))?;
    let index = build_object_manager_index(&edge)?;
    if !index
        .orphan_ids
        .contains(&"unit.issue736.orphan".to_string())
    {
        return Err("orphan object was not flagged".into());
    }
    Ok(json!({
        "orphan_ids": index.orphan_ids,
        "entry": index.entries.iter().find(|entry| entry.id == "unit.issue736.orphan")
    }))
}

fn edge_rename(project: &ObjectManagerProject) -> Result<Value, Box<dyn std::error::Error>> {
    let renamed = rename_object_and_refs(
        project,
        "unit.issue736.guard",
        "unit.issue736.guard_renamed",
    )?;
    let index = build_object_manager_index(&renamed)?;
    let old_refs = find_references(&index, "unit.issue736.guard");
    let new_refs = find_references(&index, "unit.issue736.guard_renamed");
    if !old_refs.is_empty()
        || !new_refs
            .iter()
            .any(|reference| reference.source_id == "trg_issue736_guard_death")
    {
        return Err("rename did not update trigger references".into());
    }
    Ok(json!({
        "old_ref_count": old_refs.len(),
        "new_refs": new_refs,
        "renamed_object_present": renamed.document.objects.contains_key("unit.issue736.guard_renamed")
    }))
}

fn edge_large_search(project: &ObjectManagerProject) -> Result<Value, Box<dyn std::error::Error>> {
    let mut large = project.clone();
    for index in 0..1500 {
        let id = format!("unit.issue736.worker_{index:04}");
        large.document.upsert_object(
            EditorObject::new(&id, "Unit", None, Transform::IDENTITY)
                .with_property("object.display_name", format!("Worker {index:04}")),
        )?;
    }
    let manager = build_object_manager_index(&large)?;
    let start = Instant::now();
    let hits = search_entries(&manager, "worker_1499", 4);
    let elapsed_ms = start.elapsed().as_millis();
    if hits.len() != 1 || hits[0].id != "unit.issue736.worker_1499" || elapsed_ms > 250 {
        return Err(format!(
            "large project search failed hits={} elapsed_ms={elapsed_ms}",
            hits.len()
        )
        .into());
    }
    Ok(json!({
        "entry_count": manager.entries.len(),
        "elapsed_ms": elapsed_ms,
        "hit_ids": hits.into_iter().map(|entry| entry.id).collect::<Vec<_>>()
    }))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn reset_root(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    fs::create_dir_all(root)?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    Ok(litd_hash::sha256_hex(&fs::read(path)?))
}
