//! WGSL shader authoring records, variants, hot-reload stamps, and binding validation.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};

use crate::EngineError;

pub const SHADER_SYSTEM_WGSL: &str = include_str!("shader_system.wgsl");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderSystemSettings {
    pub max_variants: usize,
}

impl Default for ShaderSystemSettings {
    fn default() -> Self {
        Self { max_variants: 256 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderSource {
    pub id: String,
    pub source: String,
    pub entry_points: ShaderEntryPoints,
    pub bindings: Vec<BindingDecl>,
    pub variant_axes: Vec<VariantAxis>,
    pub path: Option<PathBuf>,
}

impl ShaderSource {
    pub fn new(
        id: impl Into<String>,
        source: impl Into<String>,
        entry_points: ShaderEntryPoints,
    ) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            entry_points,
            bindings: Vec::new(),
            variant_axes: Vec::new(),
            path: None,
        }
    }

    pub fn with_bindings(mut self, bindings: Vec<BindingDecl>) -> Self {
        self.bindings = bindings;
        self
    }

    pub fn with_variant_axes(mut self, variant_axes: Vec<VariantAxis>) -> Self {
        self.variant_axes = variant_axes;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderEntryPoints {
    pub vertex: Option<String>,
    pub fragment: Option<String>,
    pub compute: Option<String>,
}

impl ShaderEntryPoints {
    pub fn graphics(vertex: impl Into<String>, fragment: impl Into<String>) -> Self {
        Self {
            vertex: Some(vertex.into()),
            fragment: Some(fragment.into()),
            compute: None,
        }
    }

    pub fn compute(compute: impl Into<String>) -> Self {
        Self {
            vertex: None,
            fragment: None,
            compute: Some(compute.into()),
        }
    }

    fn names(&self) -> Vec<(&'static str, &str)> {
        let mut names = Vec::new();
        if let Some(name) = &self.vertex {
            names.push(("vertex", name.as_str()));
        }
        if let Some(name) = &self.fragment {
            names.push(("fragment", name.as_str()));
        }
        if let Some(name) = &self.compute {
            names.push(("compute", name.as_str()));
        }
        names
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingKind {
    UniformBuffer,
    StorageBuffer { read_only: bool },
    Texture,
    Sampler,
    StorageTexture,
}

impl BindingKind {
    fn wgsl_marker(self) -> &'static str {
        match self {
            Self::UniformBuffer => "var<uniform>",
            Self::StorageBuffer { .. } => "var<storage",
            Self::Texture => "texture_",
            Self::Sampler => "sampler",
            Self::StorageTexture => "texture_storage",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingDecl {
    pub group: u32,
    pub binding: u32,
    pub visibility: Vec<ShaderStage>,
    pub kind: BindingKind,
}

impl BindingDecl {
    pub fn new(group: u32, binding: u32, visibility: Vec<ShaderStage>, kind: BindingKind) -> Self {
        Self {
            group,
            binding,
            visibility,
            kind,
        }
    }

    pub fn label(&self) -> String {
        format!("group={},binding={}", self.group, self.binding)
    }

    fn same_signature(&self, other: &Self) -> bool {
        self.group == other.group
            && self.binding == other.binding
            && self.kind == other.kind
            && self.visibility.len() == other.visibility.len()
            && self
                .visibility
                .iter()
                .all(|stage| other.visibility.contains(stage))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantAxis {
    pub name: String,
    pub values: Vec<String>,
}

impl VariantAxis {
    pub fn new(name: impl Into<String>, values: Vec<impl Into<String>>) -> Self {
        Self {
            name: name.into(),
            values: values.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderVariant {
    pub key: String,
    pub defines: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotReloadStamp {
    pub path: PathBuf,
    pub modified_ms: u128,
    pub source_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderReloadDecision {
    pub changed: bool,
    pub previous_hash: String,
    pub current_hash: String,
}

impl ShaderReloadDecision {
    pub fn compare(previous: &HotReloadStamp, current: &HotReloadStamp) -> Self {
        Self {
            changed: previous.modified_ms != current.modified_ms
                || previous.source_hash != current.source_hash,
            previous_hash: previous.source_hash.clone(),
            current_hash: current.source_hash.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderPlan {
    pub id: String,
    pub source_hash: String,
    pub variants: Vec<ShaderVariant>,
    pub bindings: Vec<BindingDecl>,
    pub entry_points: ShaderEntryPoints,
    pub hot_reload: Option<HotReloadStamp>,
}

pub fn load_wgsl_file(
    id: impl Into<String>,
    path: impl AsRef<Path>,
    entry_points: ShaderEntryPoints,
    bindings: Vec<BindingDecl>,
    variant_axes: Vec<VariantAxis>,
) -> Result<ShaderSource, EngineError> {
    let path = path.as_ref();
    let source = fs::read_to_string(path)
        .map_err(|err| invalid_shader_error("path", format!("failed to read WGSL: {err}")))?;
    Ok(ShaderSource {
        id: id.into(),
        source,
        entry_points,
        bindings,
        variant_axes,
        path: Some(path.to_path_buf()),
    })
}

pub fn build_shader_plan(
    settings: ShaderSystemSettings,
    shader: &ShaderSource,
    pipeline_layout: &[BindingDecl],
) -> Result<ShaderPlan, EngineError> {
    validate_shader(settings, shader)?;
    validate_shader_bindings(shader, pipeline_layout)?;
    let variants = expand_variants(settings, &shader.variant_axes)?;
    let source_hash = shader_fingerprint(&shader.source);
    let hot_reload = match &shader.path {
        Some(path) => Some(read_hot_reload_stamp(path, &source_hash)?),
        None => None,
    };

    Ok(ShaderPlan {
        id: shader.id.clone(),
        source_hash,
        variants,
        bindings: shader.bindings.clone(),
        entry_points: shader.entry_points.clone(),
        hot_reload,
    })
}

pub fn validate_shader_bindings(
    shader: &ShaderSource,
    pipeline_layout: &[BindingDecl],
) -> Result<(), EngineError> {
    let compact_source = compact_wgsl(&shader.source);
    for binding in &shader.bindings {
        if !source_declares_binding(&compact_source, binding) {
            return binding_error(
                shader,
                binding,
                "WGSL source does not declare the expected group/binding/kind",
            );
        }
        let Some(layout_binding) = pipeline_layout
            .iter()
            .find(|item| item.group == binding.group && item.binding == binding.binding)
        else {
            return binding_error(shader, binding, "missing from pipeline layout");
        };
        if !binding.same_signature(layout_binding) {
            return binding_error(
                shader,
                binding,
                "pipeline layout signature does not match WGSL",
            );
        }
    }
    for layout_binding in pipeline_layout {
        if !shader.bindings.iter().any(|binding| {
            binding.group == layout_binding.group && binding.binding == layout_binding.binding
        }) {
            return binding_error(
                shader,
                layout_binding,
                "pipeline layout declares an extra binding",
            );
        }
    }
    Ok(())
}

pub fn shader_fingerprint(source: &str) -> String {
    litd_hash::fnv1a64_hex(source.as_bytes())
}

fn validate_shader(
    settings: ShaderSystemSettings,
    shader: &ShaderSource,
) -> Result<(), EngineError> {
    if settings.max_variants == 0 {
        return invalid_shader("settings.max_variants", "must be greater than zero");
    }
    if shader.id.trim().is_empty() {
        return invalid_shader("shader.id", "must be non-empty");
    }
    if shader.source.trim().is_empty() {
        return invalid_shader("shader.source", "must be non-empty WGSL");
    }
    let compact_source = compact_wgsl(&shader.source);
    let entry_points = shader.entry_points.names();
    if entry_points.is_empty() {
        return invalid_shader(
            "shader.entry_points",
            "must declare at least one entry point",
        );
    }
    for (stage, name) in entry_points {
        validate_identifier(format!("shader.entry_points.{stage}"), name)?;
        if !compact_source.contains(&format!("fn{name}(")) {
            return invalid_shader(
                format!("shader.entry_points.{stage}"),
                "entry point function is missing from WGSL source",
            );
        }
    }
    validate_bindings_unique(&shader.bindings)?;
    for axis in &shader.variant_axes {
        validate_axis(axis)?;
    }
    Ok(())
}

fn validate_bindings_unique(bindings: &[BindingDecl]) -> Result<(), EngineError> {
    let mut seen = BTreeSet::new();
    for binding in bindings {
        if binding.visibility.is_empty() {
            return invalid_shader(binding.label(), "binding visibility must not be empty");
        }
        if !seen.insert((binding.group, binding.binding)) {
            return invalid_shader(binding.label(), "duplicate binding declaration");
        }
    }
    Ok(())
}

fn validate_axis(axis: &VariantAxis) -> Result<(), EngineError> {
    validate_identifier("variant_axis.name", &axis.name)?;
    if axis.values.is_empty() {
        return invalid_shader(format!("variant_axis.{}", axis.name), "must have values");
    }
    let mut seen = BTreeSet::new();
    for value in &axis.values {
        validate_identifier(format!("variant_axis.{}.value", axis.name), value)?;
        if !seen.insert(value) {
            return invalid_shader(format!("variant_axis.{}", axis.name), "duplicate value");
        }
    }
    Ok(())
}

fn validate_identifier(field: impl Into<String>, value: &str) -> Result<(), EngineError> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return invalid_shader(field, "identifier must be non-empty");
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return invalid_shader(field, "identifier must start with a letter or underscore");
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        return invalid_shader(field, "identifier must be ASCII alphanumeric or underscore");
    }
    Ok(())
}

fn expand_variants(
    settings: ShaderSystemSettings,
    axes: &[VariantAxis],
) -> Result<Vec<ShaderVariant>, EngineError> {
    let mut variants = vec![ShaderVariant {
        key: "base".to_string(),
        defines: BTreeMap::new(),
    }];
    for axis in axes {
        let mut next = Vec::new();
        for variant in &variants {
            for value in &axis.values {
                let mut defines = variant.defines.clone();
                defines.insert(axis.name.clone(), value.clone());
                next.push(ShaderVariant {
                    key: variant_key(&defines),
                    defines,
                });
            }
        }
        variants = next;
        if variants.len() > settings.max_variants {
            return invalid_shader(
                "variant_axes",
                "expanded variant count exceeds max_variants",
            );
        }
    }
    Ok(variants)
}

fn variant_key(defines: &BTreeMap<String, String>) -> String {
    defines
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(";")
}

fn source_declares_binding(compact_source: &str, binding: &BindingDecl) -> bool {
    let token = format!("@group({})@binding({})", binding.group, binding.binding);
    let Some(start) = compact_source.find(&token) else {
        return false;
    };
    let tail = &compact_source[start..compact_source.len().min(start + 220)];
    tail.contains(binding.kind.wgsl_marker())
}

fn compact_wgsl(source: &str) -> String {
    source.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn read_hot_reload_stamp(path: &Path, source_hash: &str) -> Result<HotReloadStamp, EngineError> {
    let metadata = fs::metadata(path).map_err(|err| {
        invalid_shader_error("path", format!("failed to stat WGSL for hot-reload: {err}"))
    })?;
    let modified = metadata.modified().map_err(|err| {
        invalid_shader_error("path", format!("failed to read modified time: {err}"))
    })?;
    let modified_ms = modified
        .duration_since(UNIX_EPOCH)
        .map_err(|err| {
            invalid_shader_error("path", format!("modified time predates epoch: {err}"))
        })?
        .as_millis();
    Ok(HotReloadStamp {
        path: path.to_path_buf(),
        modified_ms,
        source_hash: source_hash.to_string(),
    })
}

fn binding_error<T>(
    shader: &ShaderSource,
    binding: &BindingDecl,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(EngineError::Rendering(
        litd_engine::error::RenderingError::ShaderBindingValidationFailed {
            shader_id: shader.id.clone(),
            binding: binding.label(),
            reason: reason.into(),
        },
    ))
}

fn invalid_shader<T>(
    field: impl Into<String>,
    reason: impl Into<String>,
) -> Result<T, EngineError> {
    Err(invalid_shader_error(field, reason))
}

fn invalid_shader_error(field: impl Into<String>, reason: impl Into<String>) -> EngineError {
    EngineError::Rendering(litd_engine::error::RenderingError::ShaderInvalidConfig {
        field: field.into(),
        reason: reason.into(),
    })
}

#[cfg(test)]
#[path = "shader_tests.rs"]
mod tests;
