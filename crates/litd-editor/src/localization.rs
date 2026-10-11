use std::collections::{BTreeMap, BTreeSet};

use litd_engine::EngineError;
use serde::{Deserialize, Serialize};

use crate::history::stable_fingerprint;

pub const LOCALIZATION_SCHEMA_VERSION: u32 = 1;
pub const LOCALIZATION_SOURCE_ISSUE: u32 = 1219;

#[path = "localization_support.rs"]
mod localization_support;
pub use localization_support::{
    export_translation_bundle, import_translation_bundle, load_string_table, measure_text_layout,
    write_string_table,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextDirection {
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizedStringTable {
    pub schema_version: u32,
    pub source_issue: u32,
    pub default_locale: String,
    pub active_locale: String,
    pub entries: BTreeMap<String, LocalizedStringEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizedStringEntry {
    pub key: String,
    pub description: String,
    pub default_text: String,
    pub placeholders: Vec<String>,
    pub translations: BTreeMap<String, LocalizedText>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizedText {
    pub locale: String,
    pub text: String,
    pub direction: TextDirection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizedStringReadback {
    pub key: String,
    pub requested_locale: String,
    pub resolved_locale: String,
    pub text: String,
    pub direction: TextDirection,
    pub placeholders: Vec<String>,
    pub fallback_used: bool,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationBundle {
    pub schema_version: u32,
    pub source_locale: String,
    pub target_locale: String,
    pub units: Vec<TranslationUnit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationUnit {
    pub key: String,
    pub description: String,
    pub source_text: String,
    pub target_text: String,
    pub placeholders: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationImportReport {
    pub target_locale: String,
    pub updated_count: usize,
    pub keys: Vec<String>,
    pub before_fingerprint: String,
    pub after_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextLayoutProbe {
    pub direction: TextDirection,
    pub max_chars_per_line: usize,
    pub line_count: usize,
    pub max_line_chars: usize,
    pub overflow: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StringTableWriteReadback {
    pub path: String,
    pub bytes: u64,
    pub fingerprint: String,
}

impl LocalizedStringTable {
    pub fn new(default_locale: impl Into<String>) -> Result<Self, EngineError> {
        let default_locale = default_locale.into();
        validate_locale_id("default_locale", &default_locale)?;
        Ok(Self {
            schema_version: LOCALIZATION_SCHEMA_VERSION,
            source_issue: LOCALIZATION_SOURCE_ISSUE,
            active_locale: default_locale.clone(),
            default_locale,
            entries: BTreeMap::new(),
        })
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if self.schema_version != LOCALIZATION_SCHEMA_VERSION
            || self.source_issue != LOCALIZATION_SOURCE_ISSUE
        {
            return invalid_localization("schema", "unexpected localization table identity");
        }
        validate_locale_id("default_locale", &self.default_locale)?;
        validate_locale_id("active_locale", &self.active_locale)?;
        for (key, entry) in &self.entries {
            if key != &entry.key {
                return invalid_localization(
                    "entry.key",
                    format!("map key {key} does not match entry key {}", entry.key),
                );
            }
            entry.validate(&self.default_locale)?;
        }
        Ok(())
    }

    pub fn stable_fingerprint(&self) -> Result<String, EngineError> {
        self.validate()?;
        stable_fingerprint(self)
    }

    pub fn upsert_default(
        &mut self,
        key: impl Into<String>,
        default_text: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<(), EngineError> {
        let key = key.into();
        let default_text = default_text.into();
        validate_key(&key)?;
        validate_text("default_text", &default_text)?;
        let placeholders = extract_placeholders(&default_text)?;
        let mut next = self.clone();
        let translations = next
            .entries
            .get(&key)
            .map(|entry| entry.translations.clone())
            .unwrap_or_default();
        let entry = LocalizedStringEntry {
            key: key.clone(),
            description: description.into(),
            default_text,
            placeholders,
            translations,
        };
        next.entries.insert(key, entry);
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn set_translation(
        &mut self,
        key: &str,
        locale: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<(), EngineError> {
        let locale = locale.into();
        let text = text.into();
        validate_locale_id("translation.locale", &locale)?;
        if locale == self.default_locale {
            return invalid_localization(
                "translation.locale",
                "default locale text must be edited through upsert_default",
            );
        }
        validate_text("translation.text", &text)?;
        let mut next = self.clone();
        let entry = next
            .entries
            .get_mut(key)
            .ok_or_else(|| invalid_localization_error("entry.key", format!("missing key {key}")))?;
        validate_placeholder_match(&entry.placeholders, &text)?;
        entry.translations.insert(
            locale.clone(),
            LocalizedText {
                locale: locale.clone(),
                text,
                direction: direction_for_locale(&locale),
            },
        );
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn set_active_locale(&mut self, locale: impl Into<String>) -> Result<(), EngineError> {
        let locale = locale.into();
        validate_locale_id("active_locale", &locale)?;
        let mut next = self.clone();
        next.active_locale = locale;
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn render_active(&self, key: &str) -> Result<LocalizedStringReadback, EngineError> {
        self.render_locale(key, &self.active_locale)
    }

    pub fn render_locale(
        &self,
        key: &str,
        locale: &str,
    ) -> Result<LocalizedStringReadback, EngineError> {
        self.validate()?;
        validate_locale_id("render.locale", locale)?;
        let entry = self.entries.get(key).ok_or_else(|| {
            invalid_localization_error("render.key", format!("missing key {key}"))
        })?;
        if locale == self.default_locale {
            return Ok(entry.readback(locale, &self.default_locale, false, None));
        }
        match entry.translations.get(locale) {
            Some(text) => Ok(LocalizedStringReadback {
                key: key.to_string(),
                requested_locale: locale.to_string(),
                resolved_locale: locale.to_string(),
                text: text.text.clone(),
                direction: text.direction,
                placeholders: entry.placeholders.clone(),
                fallback_used: false,
                diagnostic: None,
            }),
            None => Ok(entry.readback(
                locale,
                &self.default_locale,
                true,
                Some(format!(
                    "missing translation for key {key} locale {locale}; using default locale {}",
                    self.default_locale
                )),
            )),
        }
    }

    pub fn render_locale_with_args(
        &self,
        key: &str,
        locale: &str,
        args: &BTreeMap<String, String>,
    ) -> Result<LocalizedStringReadback, EngineError> {
        let mut readback = self.render_locale(key, locale)?;
        validate_args(&readback.placeholders, args)?;
        readback.text = substitute_placeholders(&readback.text, args)?;
        Ok(readback)
    }
}

impl LocalizedStringEntry {
    fn validate(&self, default_locale: &str) -> Result<(), EngineError> {
        validate_key(&self.key)?;
        validate_text("entry.default_text", &self.default_text)?;
        if self.placeholders != extract_placeholders(&self.default_text)? {
            return invalid_localization(
                "entry.placeholders",
                format!("placeholder list drifted for key {}", self.key),
            );
        }
        for (locale, text) in &self.translations {
            if locale == default_locale {
                return invalid_localization(
                    "entry.translations",
                    "default locale cannot be stored as a translation",
                );
            }
            if locale != &text.locale {
                return invalid_localization(
                    "translation.locale",
                    format!("map locale {locale} does not match stored {}", text.locale),
                );
            }
            validate_locale_id("translation.locale", locale)?;
            validate_placeholder_match(&self.placeholders, &text.text)?;
        }
        Ok(())
    }

    fn readback(
        &self,
        requested_locale: &str,
        default_locale: &str,
        fallback_used: bool,
        diagnostic: Option<String>,
    ) -> LocalizedStringReadback {
        LocalizedStringReadback {
            key: self.key.clone(),
            requested_locale: requested_locale.to_string(),
            resolved_locale: default_locale.to_string(),
            text: self.default_text.clone(),
            direction: direction_for_locale(default_locale),
            placeholders: self.placeholders.clone(),
            fallback_used,
            diagnostic,
        }
    }
}

fn validate_key(key: &str) -> Result<(), EngineError> {
    if key.trim().is_empty() {
        return invalid_localization("key", "must be non-empty");
    }
    if !key
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return invalid_localization(
            "key",
            format!("{key} must use only ASCII alnum, '.', '_', '-'"),
        );
    }
    Ok(())
}

pub(crate) fn validate_locale_id(field: &str, locale: &str) -> Result<(), EngineError> {
    if locale.trim().is_empty() || locale.contains('_') {
        return invalid_localization(field, "locale must be a non-empty BCP-47 style id");
    }
    let parts = locale.split('-').collect::<Vec<_>>();
    if parts[0].len() < 2
        || parts[0].len() > 8
        || !parts[0].bytes().all(|b| b.is_ascii_alphabetic())
    {
        return invalid_localization(field, format!("invalid language subtag {locale}"));
    }
    for part in parts.iter().skip(1) {
        if part.is_empty()
            || part.len() > 8
            || !part.bytes().all(|byte| byte.is_ascii_alphanumeric())
        {
            return invalid_localization(field, format!("invalid locale subtag {locale}"));
        }
    }
    Ok(())
}

pub(crate) fn validate_text(field: &str, text: &str) -> Result<(), EngineError> {
    if text.trim().is_empty() {
        return invalid_localization(field, "text must be non-empty");
    }
    extract_placeholders(text).map(|_| ())
}

pub(crate) fn validate_placeholder_match(
    expected: &[String],
    text: &str,
) -> Result<(), EngineError> {
    let actual = extract_placeholders(text)?;
    if actual != expected {
        return invalid_localization(
            "placeholder",
            format!(
                "placeholder mismatch: expected {:?}, got {:?}",
                expected, actual
            ),
        );
    }
    Ok(())
}

fn validate_args(expected: &[String], args: &BTreeMap<String, String>) -> Result<(), EngineError> {
    let actual = args.keys().cloned().collect::<Vec<_>>();
    if actual != expected {
        return invalid_localization(
            "format.args",
            format!(
                "format args mismatch: expected {:?}, got {:?}",
                expected, actual
            ),
        );
    }
    Ok(())
}

fn extract_placeholders(text: &str) -> Result<Vec<String>, EngineError> {
    let mut names = BTreeSet::new();
    let chars = text.chars().collect::<Vec<_>>();
    let mut index = 0_usize;
    while index < chars.len() {
        match chars[index] {
            '{' => {
                let mut end = index + 1;
                while end < chars.len() && chars[end] != '}' {
                    if chars[end] == '{' {
                        return invalid_localization(
                            "placeholder",
                            "nested placeholders are invalid",
                        );
                    }
                    end += 1;
                }
                if end >= chars.len() {
                    return invalid_localization("placeholder", "unclosed placeholder");
                }
                let name = chars[index + 1..end].iter().collect::<String>();
                validate_placeholder_name(&name)?;
                names.insert(name);
                index = end;
            }
            '}' => return invalid_localization("placeholder", "unopened placeholder close brace"),
            _ => {}
        }
        index += 1;
    }
    Ok(names.into_iter().collect())
}

fn validate_placeholder_name(name: &str) -> Result<(), EngineError> {
    if name.is_empty() {
        return invalid_localization("placeholder", "placeholder name must be non-empty");
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return invalid_localization(
            "placeholder",
            format!("placeholder {name} must use ASCII alnum or '_'"),
        );
    }
    Ok(())
}

fn substitute_placeholders(
    text: &str,
    args: &BTreeMap<String, String>,
) -> Result<String, EngineError> {
    let mut output = text.to_string();
    for (key, value) in args {
        output = output.replace(&format!("{{{key}}}"), value);
    }
    if output.contains('{') || output.contains('}') {
        return invalid_localization("format.output", "unresolved placeholder remained in output");
    }
    Ok(output)
}

fn direction_for_locale(locale: &str) -> TextDirection {
    let language = locale
        .split('-')
        .next()
        .unwrap_or(locale)
        .to_ascii_lowercase();
    match language.as_str() {
        "ar" | "fa" | "he" | "iw" | "ur" | "yi" => TextDirection::Rtl,
        _ => TextDirection::Ltr,
    }
}

pub(crate) fn invalid_localization<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_localization_error(field, reason))
}

pub(crate) fn invalid_localization_error(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> EngineError {
    EngineError::UiRenderInvalidConfig {
        field: format!("editor.localization.{}", field.into()),
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "localization_render_ui_tests.rs"]
mod render_ui_tests;
#[cfg(test)]
#[path = "localization_tests.rs"]
mod tests;
