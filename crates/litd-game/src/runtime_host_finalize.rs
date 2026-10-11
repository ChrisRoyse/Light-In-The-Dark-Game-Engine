impl RuntimeHostApp {
    fn shutdown_state_writer(&mut self) -> Result<(), EngineError> {
        match self.state_writer.take() {
            Some(mut writer) => writer.shutdown(),
            None => Ok(()),
        }
    }

    fn final_report(self) -> Result<RuntimeHostFinalReport, EngineError> {
        let final_state = self.last_state.ok_or_else(|| {
            runtime_host_error(
                "state.final",
                "runtime host exited without writing any runtime state",
            )
        })?;
        Ok(RuntimeHostFinalReport {
            final_state,
            state_readback: self.last_state_readback,
            replay: self.last_replay_snapshot,
        })
    }
}
