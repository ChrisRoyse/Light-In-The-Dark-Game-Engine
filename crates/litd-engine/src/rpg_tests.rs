use crate::rpg::{DerivedStatFormulaTable, RPG_STATS_SCHEMA};

#[test]
fn issue960_default_formula_table_declares_current_schema() {
    let table = DerivedStatFormulaTable::mud_defaults();
    assert_eq!(table.schema, RPG_STATS_SCHEMA);
    assert_eq!(table.resistances.len(), 9);
}
