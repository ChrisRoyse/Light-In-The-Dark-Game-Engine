//! Test-only forged-rule controls for runtime defense-in-depth falsifiers.

use super::EconomyWorld;

impl EconomyWorld {
    pub(in crate::game_runtime) fn forge_structure_duration_for_test(
        &mut self,
        struct_id: &str,
        secs: u32,
    ) {
        self.ruleset
            .structures
            .iter_mut()
            .find(|structure| structure.struct_id == struct_id)
            .expect("forged structure exists")
            .build_time_secs = secs;
    }

    pub(in crate::game_runtime) fn forge_unit_duration_for_test(
        &mut self,
        unit_id: &str,
        secs: u32,
    ) {
        self.ruleset
            .units
            .iter_mut()
            .find(|unit| unit.unit_id == unit_id)
            .expect("forged unit exists")
            .build_time_secs = secs;
    }
}
