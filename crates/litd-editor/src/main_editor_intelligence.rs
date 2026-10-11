use std::{env, path::PathBuf};

use egui::RichText;
use litd_editor::editor_intelligence_surfaces::{
    EditorIntelligenceSurfaceDeck, EditorIntelligenceSurfacePanel,
    EditorIntelligenceSurfaceWriteback, parse_editor_intelligence_surface_deck,
    read_editor_intelligence_surface_deck,
};
use litd_engine::EngineError;
use serde_json::Value;

use super::*;

const ENV_ASTER_DIR: &str = "LITD_EDITOR_INTELLIGENCE_ASTER_DIR";
const ENV_DECK_ID: &str = "LITD_EDITOR_INTELLIGENCE_DECK_ID";

#[derive(Debug, Clone)]
pub(in crate::main_editor_ui) struct EditorIntelligenceUiRuntime {
    loaded: Option<LoadedEditorIntelligenceDeck>,
}

#[derive(Debug, Clone)]
struct LoadedEditorIntelligenceDeck {
    deck: EditorIntelligenceSurfaceDeck,
    readback: EditorIntelligenceSurfaceWriteback,
    aster_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct EditorIntelligenceUiEvidence {
    pub status: String,
    pub configured: bool,
    pub deck_id: String,
    pub aster_dir: String,
    pub readback_sha256: String,
    pub aster_seq: u64,
    pub graph_rows: usize,
    pub surface_count: usize,
    pub observation_count: usize,
    pub panel_ids: String,
    pub surface_titles: String,
    pub source_tools: String,
    pub metric_count: usize,
    pub focus_cx_id_count: usize,
    pub warning_count: usize,
}

impl EditorIntelligenceUiRuntime {
    pub(in crate::main_editor_ui) fn from_env() -> Result<Self, EngineError> {
        let aster_dir = env_path(ENV_ASTER_DIR)?;
        let deck_id = env_text(ENV_DECK_ID)?;
        Self::from_env_values(aster_dir, deck_id)
    }

    fn from_env_values(
        aster_dir: Option<PathBuf>,
        deck_id: Option<String>,
    ) -> Result<Self, EngineError> {
        match (aster_dir, deck_id) {
            (None, None) => Ok(Self { loaded: None }),
            (Some(aster_dir), Some(deck_id)) => {
                let readback = read_editor_intelligence_surface_deck(&aster_dir, &deck_id)?
                    .ok_or_else(|| {
                        intelligence_error(
                            "aster_deck",
                            format!("deck {deck_id} was not found in {}", aster_dir.display()),
                        )
                    })?;
                let deck = parse_editor_intelligence_surface_deck(&readback.bytes)?;
                if readback.surface_count != deck.surface_count
                    || readback.observation_count != deck.observation_count
                {
                    return Err(intelligence_error(
                        "aster_readback",
                        "deck counters differed between Aster readback and parsed bytes",
                    ));
                }
                Ok(Self {
                    loaded: Some(LoadedEditorIntelligenceDeck {
                        deck,
                        readback,
                        aster_dir,
                    }),
                })
            }
            _ => Err(intelligence_error(
                "env",
                format!("{ENV_ASTER_DIR} and {ENV_DECK_ID} must be set together"),
            )),
        }
    }

    pub(in crate::main_editor_ui) fn evidence(&self) -> EditorIntelligenceUiEvidence {
        let Some(loaded) = &self.loaded else {
            return EditorIntelligenceUiEvidence::not_configured();
        };
        let deck = &loaded.deck;
        EditorIntelligenceUiEvidence {
            status: "loaded".to_string(),
            configured: true,
            deck_id: deck.deck_id.clone(),
            aster_dir: loaded.aster_dir.display().to_string(),
            readback_sha256: loaded.readback.sha256.clone(),
            aster_seq: loaded.readback.seq,
            graph_rows: loaded.readback.graph_rows,
            surface_count: deck.surface_count,
            observation_count: deck.observation_count,
            panel_ids: join_panel_values(deck, |panel| panel.panel_id.as_str()),
            surface_titles: join_panel_values(deck, |panel| panel.title.as_str()),
            source_tools: deck
                .surfaces
                .iter()
                .flat_map(|panel| panel.source_tools.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join("|"),
            metric_count: deck.surfaces.iter().map(|panel| panel.metrics.len()).sum(),
            focus_cx_id_count: deck
                .surfaces
                .iter()
                .map(|panel| panel.focus_cx_ids.len())
                .sum(),
            warning_count: deck.surfaces.iter().map(|panel| panel.warnings.len()).sum(),
        }
    }
}

impl EditorIntelligenceUiEvidence {
    fn not_configured() -> Self {
        Self {
            status: "not_configured".to_string(),
            configured: false,
            deck_id: "none".to_string(),
            aster_dir: "none".to_string(),
            readback_sha256: "none".to_string(),
            aster_seq: 0,
            graph_rows: 0,
            surface_count: 0,
            observation_count: 0,
            panel_ids: "none".to_string(),
            surface_titles: "none".to_string(),
            source_tools: "none".to_string(),
            metric_count: 0,
            focus_cx_id_count: 0,
            warning_count: 0,
        }
    }
}

impl EditorShellRuntime {
    pub(in crate::main_editor_ui) fn intelligence_surfaces_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Calyx Intelligence").strong().color(TEXT));
        let Some(loaded) = &self.editor_intelligence.loaded else {
            ui.label(RichText::new("not loaded").size(11.0).color(TEXT_DIM));
            return;
        };
        let deck = &loaded.deck;
        ui.label(
            RichText::new(format!(
                "{} / {} surfaces / sha {}",
                deck.deck_id,
                deck.surface_count,
                short_digest(&loaded.readback.sha256)
            ))
            .size(11.0)
            .color(TEXT_DIM),
        );
        for panel in &deck.surfaces {
            ui.add_space(3.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&panel.title).size(12.0).strong().color(GOLD));
                ui.label(
                    RichText::new(format!("{:?}", panel.dock))
                        .size(10.0)
                        .color(TEXT_DIM),
                );
            });
            ui.label(RichText::new(metric_summary(panel)).size(11.0).color(TEXT));
            if !panel.focus_cx_ids.is_empty() {
                ui.label(
                    RichText::new(format!("focus {}", short_list(&panel.focus_cx_ids)))
                        .size(10.0)
                        .color(TEXT_DIM),
                );
            }
            if !panel.warnings.is_empty() {
                ui.label(
                    RichText::new(short_list(&panel.warnings))
                        .size(10.0)
                        .color(ERROR),
                );
            }
        }
    }
}

fn env_path(name: &str) -> Result<Option<PathBuf>, EngineError> {
    let Some(value) = env::var_os(name) else {
        return Ok(None);
    };
    let text = value.to_string_lossy();
    if text.trim().is_empty() {
        return Err(intelligence_error(
            "env",
            format!("{name} must not be empty"),
        ));
    }
    Ok(Some(PathBuf::from(value)))
}

fn env_text(name: &str) -> Result<Option<String>, EngineError> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => Err(intelligence_error(
            "env",
            format!("{name} must not be empty"),
        )),
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(intelligence_error("env", format!("{name}: {error}"))),
    }
}

fn join_panel_values(
    deck: &EditorIntelligenceSurfaceDeck,
    value: impl Fn(&EditorIntelligenceSurfacePanel) -> &str,
) -> String {
    deck.surfaces
        .iter()
        .map(value)
        .collect::<Vec<_>>()
        .join("|")
}

fn metric_summary(panel: &EditorIntelligenceSurfacePanel) -> String {
    panel
        .metrics
        .iter()
        .take(3)
        .map(|metric| format!("{}={}", metric.name, value_label(&metric.value)))
        .collect::<Vec<_>>()
        .join("  ")
}

fn value_label(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }
    if let Some(number) = value.as_f64() {
        return format!("{number:.3}");
    }
    value.to_string()
}

fn short_list(items: &[String]) -> String {
    items
        .iter()
        .take(3)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

fn short_digest(value: &str) -> String {
    value.chars().take(12).collect()
}

fn intelligence_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.intelligence_surfaces.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use litd_editor::{
        editor_intelligence_surfaces::{
            EditorIntelligenceObservation, build_editor_intelligence_surface_deck,
            write_editor_intelligence_surface_deck,
        },
        mcp_tools::default_structured_intelligence_mcp_surface_manifest,
    };
    use serde_json::json;

    use super::*;

    #[test]
    fn issue1420_editor_ui_reports_not_configured_without_env_pair() {
        let runtime = EditorIntelligenceUiRuntime::from_env_values(None, None).unwrap();
        let evidence = runtime.evidence();

        assert_eq!(evidence.status, "not_configured");
        assert!(!evidence.configured);
        assert_eq!(evidence.surface_count, 0);
        assert_eq!(evidence.panel_ids, "none");
    }

    #[test]
    fn issue1420_editor_ui_requires_complete_env_pair() {
        let error = EditorIntelligenceUiRuntime::from_env_values(
            Some(PathBuf::from("target/issue1420-missing-deck")),
            None,
        )
        .unwrap_err();

        assert_ui_error(
            error,
            "editor.intelligence_surfaces.env",
            "must be set together",
        );
    }

    #[test]
    fn issue1420_editor_ui_loads_aster_deck_for_panel_evidence() {
        let root = temp_root("issue1420-editor-ui");
        fs::create_dir_all(&root).unwrap();
        let deck = test_deck();
        let writeback = write_editor_intelligence_surface_deck(&root, &root, &deck).unwrap();

        let runtime = EditorIntelligenceUiRuntime::from_env_values(
            Some(root.to_path_buf()),
            Some(deck.deck_id),
        )
        .unwrap();
        let evidence = runtime.evidence();

        assert_eq!(evidence.status, "loaded");
        assert!(evidence.configured);
        assert_eq!(evidence.readback_sha256, writeback.sha256);
        assert_eq!(evidence.surface_count, 5);
        assert_eq!(evidence.observation_count, 6);
        assert!(evidence.surface_titles.contains("Abundance"));
        assert!(evidence.surface_titles.contains("What If"));
        assert!(evidence.source_tools.contains("calyx.guard.check"));
        assert!(evidence.metric_count >= 18);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn issue1420_editor_ui_fails_on_missing_aster_deck() {
        let root = temp_root("issue1420-editor-ui-missing");
        fs::create_dir_all(&root).unwrap();
        let error = EditorIntelligenceUiRuntime::from_env_values(
            Some(root.to_path_buf()),
            Some("issue1420-missing".to_string()),
        )
        .unwrap_err();

        assert_ui_error(
            error,
            "editor.intelligence_surfaces.aster_deck",
            "not found",
        );
        let _ = fs::remove_dir_all(&root);
    }

    fn test_deck() -> EditorIntelligenceSurfaceDeck {
        let manifest = default_structured_intelligence_mcp_surface_manifest().unwrap();
        build_editor_intelligence_surface_deck(
            "issue1420-editor-ui-deck",
            "issue1420_structured",
            "ui integration test Aster deck",
            &manifest,
            observations(),
        )
        .unwrap()
    }

    fn observations() -> Vec<EditorIntelligenceObservation> {
        vec![
            observation(
                "calyx.abundance",
                json!({"n": 60, "pairs": 1770, "materialized": 60, "panel_size": 1}),
            ),
            observation(
                "calyx.bits",
                json!({"anchor": "test_pass", "panel_sufficiency": 0.02, "per_slot": [{
                    "slot": 8, "name": "summary.text_embedding", "bits": 0.12,
                    "state": "active", "low_signal": false
                }]}),
            ),
            observation(
                "calyx.neighbors",
                json!({"neighbors": [
                    {"cx_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "score": 1.0, "slot": 8},
                    {"cx_id": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "score": 0.98, "slot": 8}
                ]}),
            ),
            observation(
                "calyx.kernel",
                json!({"kernel_size": 1, "recall": 1.0,
                    "kernel_cx_ids": ["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"], "grounding_gaps": []}),
            ),
            observation(
                "calyx.kernel_answer",
                json!({"answer": "grounded answer", "recall": 1.0,
                    "kernel_cx_ids": ["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"], "gaps": []}),
            ),
            observation(
                "calyx.guard.check",
                json!({"verdict": "pass", "tau": 0.9, "distance": 0.0}),
            ),
        ]
    }

    fn observation(tool: &str, payload: Value) -> EditorIntelligenceObservation {
        EditorIntelligenceObservation::new(
            tool,
            format!("mcp://loopback/{tool}/issue1420"),
            payload,
        )
    }

    fn assert_ui_error(error: EngineError, field: &str, reason_contains: &str) {
        match error {
            EngineError::UiRenderInvalidConfig {
                field: actual,
                reason,
            } => {
                assert_eq!(actual, field);
                assert!(reason.contains(reason_contains), "{reason}");
            }
            other => panic!("expected UiRenderInvalidConfig, got {other:?}"),
        }
    }

    fn temp_root(label: &str) -> litd_engine::test_temp::TestTempDir {
        litd_engine::test_temp::TestTempDir::new(label)
    }
}
