use std::{collections::BTreeSet, path::Path};

use litd_engine::EngineError;

use super::{
    GenerateLoreTemplatesArgs, LoreTemplateKind, LoreTemplateRequest, LoreTemplateWardPolicy,
    invalid_mcp_story,
};

const MAX_TEMPLATES: usize = 16;
const MAX_GENERATED_ENTRIES: usize = 64;
const MAX_BLOCKED_TERMS: usize = 64;
const MAX_TEXT_LEN: usize = 4096;

pub(super) fn validate_args(args: &GenerateLoreTemplatesArgs) -> Result<(), EngineError> {
    validate_path("template_ledger_path", &args.template_ledger_path)?;
    if !crate::row_safe_ledger_id::is_row_safe_ledger_id(&args.template_ledger_id) {
        return Err(invalid_mcp_story(
            "lore_template.template_ledger_id",
            "template ledger id must be row-safe",
        ));
    }
    validate_ward(&args.ward_policy)?;
    if args.templates.is_empty() || args.templates.len() > MAX_TEMPLATES {
        return Err(invalid_mcp_story(
            "lore_template.templates",
            format!("template count must be in 1..={MAX_TEMPLATES}"),
        ));
    }
    let mut template_ids = BTreeSet::new();
    let mut total = 0usize;
    for template in &args.templates {
        validate_template(template)?;
        if !template_ids.insert(template.template_id.as_str()) {
            return Err(invalid_mcp_story(
                "lore_template.template_id",
                format!("duplicate template {}", template.template_id),
            ));
        }
        let blocked = blocked_terms(&template.theme, &args.ward_policy.blocked_terms);
        if !blocked.is_empty() {
            return Err(invalid_mcp_story(
                "lore_template.ward",
                format!("LITD-LORE-TEMPLATE-BLOCKED: {}", blocked.join("|")),
            ));
        }
        total += generated_entry_count(template);
    }
    if total == 0 || total > args.ward_policy.max_generated_entries {
        return Err(invalid_mcp_story(
            "lore_template.generated_entries",
            format!(
                "generated entry count {total} exceeds Ward max {}",
                args.ward_policy.max_generated_entries
            ),
        ));
    }
    Ok(())
}

pub(super) fn validate_path(field: &str, path: &Path) -> Result<(), EngineError> {
    if path.as_os_str().is_empty() {
        return Err(invalid_mcp_story(field, "path must be non-empty"));
    }
    if path.is_file() {
        return Err(invalid_mcp_story(
            field,
            format!(
                "{} is a file; expected a Calyx Aster vault directory",
                path.display()
            ),
        ));
    }
    Ok(())
}

pub(super) fn generated_entry_count(template: &LoreTemplateRequest) -> usize {
    match template.kind {
        LoreTemplateKind::Pantheon | LoreTemplateKind::RivalCabalSet => template.count,
        LoreTemplateKind::CreationMyth | LoreTemplateKind::FallenEmpire => 1,
    }
}

pub(super) fn template_kind_name(kind: LoreTemplateKind) -> &'static str {
    match kind {
        LoreTemplateKind::Pantheon => "pantheon",
        LoreTemplateKind::RivalCabalSet => "rival_cabal_set",
        LoreTemplateKind::CreationMyth => "creation_myth",
        LoreTemplateKind::FallenEmpire => "fallen_empire",
    }
}

pub(super) fn blocked_terms(text: &str, blocked: &[String]) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    blocked
        .iter()
        .filter(|term| lower.contains(&term.to_ascii_lowercase()))
        .cloned()
        .collect()
}

pub(super) fn symbol_prefix(value: &str) -> String {
    value.to_ascii_lowercase()
}

fn validate_template(template: &LoreTemplateRequest) -> Result<(), EngineError> {
    validate_symbol("lore_template.template_id", &template.template_id)?;
    validate_symbol("lore_template.prefix", &template.prefix)?;
    validate_single_line("lore_template.theme", &template.theme)?;
    match template.kind {
        LoreTemplateKind::Pantheon if !(2..=12).contains(&template.count) => {
            invalid_count("pantheon", "2..=12")
        }
        LoreTemplateKind::RivalCabalSet if !(2..=10).contains(&template.count) => {
            invalid_count("rival_cabal_set", "2..=10")
        }
        LoreTemplateKind::CreationMyth | LoreTemplateKind::FallenEmpire if template.count != 1 => {
            invalid_count(template_kind_name(template.kind), "exactly 1")
        }
        _ => Ok(()),
    }
}

fn validate_ward(ward: &LoreTemplateWardPolicy) -> Result<(), EngineError> {
    validate_symbol("lore_template.ward.policy_id", &ward.policy_id)?;
    if ward.max_generated_entries == 0
        || ward.max_generated_entries > MAX_GENERATED_ENTRIES
        || ward.blocked_terms.len() > MAX_BLOCKED_TERMS
    {
        return Err(invalid_mcp_story(
            "lore_template.ward",
            format!("Ward max must be in 1..={MAX_GENERATED_ENTRIES} and blocked terms bounded"),
        ));
    }
    for term in &ward.blocked_terms {
        validate_single_line("lore_template.ward.blocked_terms", term)?;
    }
    Ok(())
}

fn invalid_count(kind: &str, expected: &str) -> Result<(), EngineError> {
    Err(invalid_mcp_story(
        "lore_template.count",
        format!("{kind} count must be {expected}"),
    ))
}

fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_LEN || value.contains(['\n', '\r', '\t']) {
        return Err(invalid_mcp_story(
            field,
            "value must be non-empty bounded single-line text",
        ));
    }
    Ok(())
}

fn validate_symbol(field: &str, value: &str) -> Result<(), EngineError> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(invalid_mcp_story(field, "symbol cannot be empty"));
    };
    if !(first.is_ascii_alphabetic() || first == '_')
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(invalid_mcp_story(
            field,
            "symbol must be an ASCII identifier",
        ));
    }
    Ok(())
}
