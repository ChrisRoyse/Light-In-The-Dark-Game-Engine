use super::*;

impl AiCoAuthorRuntime {
    pub(super) fn last_error_message(&self) -> Option<&str> {
        (!self.last_error.trim().is_empty()).then_some(self.last_error.as_str())
    }

    pub(super) fn save_state(&mut self) -> Result<(), EngineError> {
        self.document.validate()?;
        let record = self.state_record();
        let bytes = serde_json::to_vec_pretty(&record).map_err(|error| {
            EngineError::SerializationFailed {
                format: "json",
                reason: format!("serialize ai co-author state: {error}"),
            }
        })?;
        let readback = write_aster_state(&self.aster_dir, &bytes)?;
        if readback.bytes != bytes {
            return invalid_ai(
                "aster.readback",
                "Aster state_json readback bytes did not match serialized editor state",
            );
        }
        self.save_count = self.save_count.saturating_add(1);
        self.last_aster_seq = readback.seq;
        self.last_aster_graph_rows = readback.graph_rows;
        self.last_aster_verified = true;
        info!(seq = readback.seq, graph_rows = readback.graph_rows, cell_key = %readback.cell_key, index_key = %readback.index_key, "ai co-author state persisted and read back from Aster");
        Ok(())
    }

    fn state_record(&self) -> AiCoAuthorStateFile {
        AiCoAuthorStateFile {
            schema_version: AI_STATE_SCHEMA_VERSION,
            record_kind: AI_STATE_RECORD_KIND.to_string(),
            capability_operation_id: self.capability_operation_id.clone(),
            readback_tool_name: self.readback_tool_name.clone(),
            prompt: self.prompt.clone(),
            document: self.document.clone(),
            outcome: self.outcome.clone(),
            transactions: self.transactions.clone(),
            last_status: self.last_status.clone(),
            last_error: self.last_error.clone(),
        }
    }

    pub(super) fn last_live_object_id(&self) -> Option<String> {
        self.transactions.iter().rev().find_map(|transaction| {
            if transaction.action != "apply_to_world" {
                return None;
            }
            let object_id = transaction.object_id.as_ref()?;
            self.document
                .objects
                .contains_key(object_id)
                .then(|| object_id.clone())
        })
    }

    pub(super) fn require_calyx_feature(&mut self) -> Result<(), EngineError> {
        self.calyx_probe_cache.refresh_blocking(&self.calyx_probe)?;
        self.calyx_probe_cache.panel().require_enabled()
    }
}
