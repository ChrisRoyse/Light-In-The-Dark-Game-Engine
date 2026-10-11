pub(crate) fn is_row_safe_ledger_id(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}
