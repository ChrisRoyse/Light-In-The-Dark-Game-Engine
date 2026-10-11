use litd_engine::EngineError;

use super::{
    TriggerEditorProject, TriggerEditorVariable, TriggerEditorVariableType,
    support::{trigger_editor_error, trigger_editor_error_value, validate_symbol},
    trigger_editor_variables::validate_trigger_editor_variable_value,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerVariablePaletteKind {
    Boolean,
    Integer,
    String,
}

impl TriggerVariablePaletteKind {
    fn metadata(self) -> (&'static str, TriggerEditorVariableType, &'static str) {
        match self {
            Self::Boolean => ("variable_bool", TriggerEditorVariableType::Boolean, "false"),
            Self::Integer => ("variable_integer", TriggerEditorVariableType::Integer, "0"),
            Self::String => (
                "variable_string",
                TriggerEditorVariableType::String,
                "value",
            ),
        }
    }
}

pub fn add_default_trigger_variable(
    project: &mut TriggerEditorProject,
    kind: TriggerVariablePaletteKind,
) -> Result<TriggerEditorVariable, EngineError> {
    project.validate()?;
    let (stem, value_type, initial_value) = kind.metadata();
    let name = next_variable_name(project, stem)?;
    let variable = TriggerEditorVariable {
        name: name.clone(),
        value_type,
        initial_value: initial_value.to_string(),
        array_values: Vec::new(),
    };
    let before = project.clone();
    project.variables.insert(name, variable.clone());
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(variable)
}

pub fn add_default_trigger_variable_array(
    project: &mut TriggerEditorProject,
    kind: TriggerVariablePaletteKind,
    size: usize,
) -> Result<TriggerEditorVariable, EngineError> {
    project.validate()?;
    let (stem, value_type, initial_value) = kind.metadata();
    validate_array_size(size)?;
    let name = next_variable_name(project, &format!("{stem}_array"))?;
    let variable = TriggerEditorVariable {
        name: name.clone(),
        value_type,
        initial_value: String::new(),
        array_values: vec![initial_value.to_string(); size],
    };
    let before = project.clone();
    project.variables.insert(name, variable.clone());
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(variable)
}

pub fn delete_trigger_variable(
    project: &mut TriggerEditorProject,
    name: &str,
) -> Result<TriggerEditorVariable, EngineError> {
    project.validate()?;
    if !project.variables.contains_key(name) {
        return trigger_editor_error(
            "delete_variable.name",
            format!("unknown trigger variable {name}"),
        );
    }
    let references = variable_reference_nodes(project, name);
    if !references.is_empty() {
        return trigger_editor_error(
            "delete_variable.references",
            format!(
                "cannot delete referenced variable {name}; nodes={}",
                references.join(",")
            ),
        );
    }
    let before = project.clone();
    let removed = project
        .variables
        .remove(name)
        .expect("variable existence checked");
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(removed)
}

pub fn update_trigger_variable_definition(
    project: &mut TriggerEditorProject,
    old_name: &str,
    new_name: &str,
    value_type: TriggerEditorVariableType,
    initial_value: &str,
) -> Result<bool, EngineError> {
    project.validate()?;
    validate_symbol("update_variable.name", new_name)?;
    if value_type == TriggerEditorVariableType::Real {
        return trigger_editor_error(
            "update_variable.value_type",
            "live variable definitions support Boolean, Integer, or String",
        );
    }
    let current = project.variables.get(old_name).cloned().ok_or_else(|| {
        trigger_editor_error_value(
            "update_variable.old_name",
            format!("unknown trigger variable {old_name}"),
        )
    })?;
    if !current.array_values.is_empty() {
        return trigger_editor_error(
            "update_variable.array",
            "array variables must be edited with the array definition API",
        );
    }
    if old_name != new_name && project.variables.contains_key(new_name) {
        return trigger_editor_error(
            "update_variable.name",
            format!("duplicate trigger variable {new_name}"),
        );
    }
    let references = variable_reference_nodes(project, old_name);
    if current.value_type != value_type && !references.is_empty() {
        return trigger_editor_error(
            "update_variable.value_type",
            format!(
                "cannot change type of referenced variable {old_name}; nodes={}",
                references.join(",")
            ),
        );
    }
    let updated = TriggerEditorVariable {
        name: new_name.to_string(),
        value_type,
        initial_value: initial_value.to_string(),
        array_values: Vec::new(),
    };
    validate_trigger_editor_variable_value(&updated, initial_value)?;
    if current == updated {
        return Ok(false);
    }
    let before = project.clone();
    project.variables.remove(old_name);
    project.variables.insert(new_name.to_string(), updated);
    if old_name != new_name {
        rewrite_variable_references(project, old_name, new_name);
    }
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(true)
}

pub fn update_trigger_variable_array_definition(
    project: &mut TriggerEditorProject,
    old_name: &str,
    new_name: &str,
    value_type: TriggerEditorVariableType,
    array_values: Vec<String>,
) -> Result<bool, EngineError> {
    project.validate()?;
    validate_symbol("update_variable_array.name", new_name)?;
    if value_type == TriggerEditorVariableType::Real {
        return trigger_editor_error(
            "update_variable_array.value_type",
            "live variable arrays support Boolean, Integer, or String",
        );
    }
    let current = project.variables.get(old_name).cloned().ok_or_else(|| {
        trigger_editor_error_value(
            "update_variable_array.old_name",
            format!("unknown trigger variable array {old_name}"),
        )
    })?;
    if current.array_values.is_empty() {
        return trigger_editor_error(
            "update_variable_array.shape",
            format!("trigger variable {old_name} is scalar"),
        );
    }
    if old_name != new_name && project.variables.contains_key(new_name) {
        return trigger_editor_error(
            "update_variable_array.name",
            format!("duplicate trigger variable {new_name}"),
        );
    }
    validate_array_size(array_values.len())?;
    let updated = TriggerEditorVariable {
        name: new_name.to_string(),
        value_type,
        initial_value: String::new(),
        array_values,
    };
    for value in &updated.array_values {
        validate_trigger_editor_variable_value(&updated, value)?;
    }
    let references = variable_reference_nodes(project, old_name);
    if current.value_type != value_type && !references.is_empty() {
        return trigger_editor_error(
            "update_variable_array.value_type",
            format!(
                "cannot change type of referenced variable array {old_name}; nodes={}",
                references.join(",")
            ),
        );
    }
    let unsafe_resize = array_reference_indices(project, old_name)
        .into_iter()
        .filter(|(_, index)| *index >= updated.array_values.len())
        .map(|(node, index)| format!("{node}[{index}]"))
        .collect::<Vec<_>>();
    if !unsafe_resize.is_empty() {
        return trigger_editor_error(
            "update_variable_array.size",
            format!(
                "cannot shrink variable array {old_name}; indexed references={}",
                unsafe_resize.join(",")
            ),
        );
    }
    if current == updated {
        return Ok(false);
    }
    let before = project.clone();
    project.variables.remove(old_name);
    project.variables.insert(new_name.to_string(), updated);
    if old_name != new_name {
        rewrite_variable_references(project, old_name, new_name);
    }
    if let Err(error) = project.validate() {
        *project = before;
        return Err(error);
    }
    Ok(true)
}

fn validate_array_size(size: usize) -> Result<(), EngineError> {
    if !(2..=256).contains(&size) {
        return trigger_editor_error(
            "variable_array.size",
            format!("array size {size} must be 2..=256"),
        );
    }
    Ok(())
}

fn next_variable_name(project: &TriggerEditorProject, stem: &str) -> Result<String, EngineError> {
    let prefix = format!("{stem}_");
    let suffix = project
        .variables
        .keys()
        .filter_map(|name| name.strip_prefix(&prefix))
        .filter_map(|value| value.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| {
            trigger_editor_error_value(
                "add_variable.name",
                format!("variable name suffix exhausted for {stem}"),
            )
        })?;
    let name = format!("{prefix}{suffix}");
    if project.variables.contains_key(&name) {
        return trigger_editor_error(
            "add_variable.name",
            format!("duplicate trigger variable {name}"),
        );
    }
    Ok(name)
}

fn variable_reference_nodes(project: &TriggerEditorProject, name: &str) -> Vec<String> {
    project
        .graphs
        .iter()
        .flat_map(|graph| {
            graph
                .events
                .iter()
                .chain(graph.conditions.iter())
                .chain(graph.actions.iter())
        })
        .filter(|node| node.variable_refs.iter().any(|reference| reference == name))
        .map(|node| node.node_id.clone())
        .collect()
}

fn array_reference_indices(project: &TriggerEditorProject, name: &str) -> Vec<(String, usize)> {
    project
        .graphs
        .iter()
        .flat_map(|graph| graph.actions.iter())
        .filter(|node| {
            node.operation == "set_variable"
                && node.args.get("variable").map(String::as_str) == Some(name)
        })
        .filter_map(|node| {
            node.args
                .get("index")
                .and_then(|index| index.parse::<usize>().ok())
                .map(|index| (node.node_id.clone(), index))
        })
        .collect()
}

fn rewrite_variable_references(project: &mut TriggerEditorProject, old_name: &str, new_name: &str) {
    for node in project.graphs.iter_mut().flat_map(|graph| {
        graph
            .events
            .iter_mut()
            .chain(graph.conditions.iter_mut())
            .chain(graph.actions.iter_mut())
    }) {
        for reference in &mut node.variable_refs {
            if reference == old_name {
                *reference = new_name.to_string();
            }
        }
        let key = match node.operation.as_str() {
            "game_flag_true" => Some("flag"),
            "set_variable" => Some("variable"),
            _ => None,
        };
        if let Some(key) = key
            && node.args.get(key).map(String::as_str) == Some(old_name)
        {
            node.args.insert(key.to_string(), new_name.to_string());
        }
    }
}
