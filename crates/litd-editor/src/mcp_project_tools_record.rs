use std::path::Path;

use super::{LitdWorldProject, ProjectLifecycleRecord};

pub(super) fn project_record(project: &LitdWorldProject, path: &Path) -> ProjectLifecycleRecord {
    ProjectLifecycleRecord {
        project_id: project.project_id.clone(),
        title: project.title.clone(),
        version: project.version.clone(),
        tags: project.tags.clone(),
        project_path: path.display().to_string(),
        document_fingerprint: project.document_fingerprint.clone(),
        revision_id: project.document.revision_id.clone(),
        object_count: project.document.objects.len(),
        unit_count: project
            .document
            .objects
            .values()
            .filter(|object| object.kind == "Unit")
            .count(),
        snapshot_label: project.snapshot_label.clone(),
    }
}
