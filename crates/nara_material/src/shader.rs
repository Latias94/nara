//! Bounded WGSL compilation for the stock 2D material interface.

use std::error::Error;
use std::fmt::{self, Write as _};
use std::sync::Arc;

use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga::{AddressSpace, ImageClass, ImageDimension, ScalarKind, ShaderStage, TypeInner};

use crate::declaration::{
    MATERIAL_BIND_GROUP, MATERIAL_UNIFORM_BINDING, MAX_MATERIAL_TEXTURES, Material2dDeclaration,
};
use crate::parameters::{
    MAX_MATERIAL_PARAMETERS, MAX_MATERIAL_UNIFORM_BYTES, MaterialParameterKind,
    MaterialParameterLayout,
};

#[doc(hidden)]
pub const __STOCK_QUAD_SHADER_SOURCE: &str = include_str!("fragment.wgsl");
const STOCK_FRAGMENT: &str = __STOCK_QUAD_SHADER_SOURCE;
const MAX_SOURCE_BYTES: usize = 1_048_576;
const MAX_DIAGNOSTIC_BYTES: usize = 8_192;
const MAX_ANNOTATED_SOURCE_BYTES: usize = 16_384;
const MAX_DIAGNOSTIC_SPANS: usize = 16;
const STOCK_ENTRY_POINTS: [(&str, ShaderStage); 3] = [
    ("vs_main", ShaderStage::Vertex),
    ("fs_main", ShaderStage::Fragment),
    ("fs_opaque", ShaderStage::Fragment),
];

/// Immutable, CPU-validated source for the stock 2D material pipeline.
///
/// This artifact retains the complete assembled WGSL and its observed interface.
/// It owns no GPU objects, device epochs, source files or publication authority.
/// Successful compilation does not prove native GPU pipeline creation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialShaderArtifact {
    source: Arc<str>,
    digest: [u8; 32],
    interface_digest: [u8; 32],
}

impl MaterialShaderArtifact {
    /// Assembles and validates a material against the actual Naga type results.
    ///
    /// `source` supplies ordinary WGSL functions, including
    /// `fn material_fragment(vec4<f32>, vec4<f32>, vec2<f32>) -> vec4<f32>`.
    /// The stock entry points are retained verbatim. The declaration generates
    /// `MaterialParameters`, `material_parameters` at group 1 binding 0, and the
    /// declared float-sampled, non-arrayed, non-multisampled 2D textures at group
    /// 1 bindings 1 through 8. Additional bound resources and entry points reject.
    /// Resource-free private globals are permitted.
    ///
    /// Both the supplied source and the complete assembled source are limited
    /// to 1 MiB. Assembly is counted before allocating its source buffer. Layouts
    /// are limited to 32 fields, 65,536 uniform bytes and eight textures. Compiler
    /// diagnostics retain at most 8,192 UTF-8 bytes.
    ///
    /// Compilation performs no filesystem or GPU work. It uses every Naga
    /// validation flag and no optional shader capabilities.
    ///
    /// # Errors
    ///
    /// Returns an error for empty or excessive source, WGSL parse or validation
    /// failure, or a mismatch between the compiled interface and the declaration.
    pub fn compile(
        declaration: &Material2dDeclaration,
        source: &str,
    ) -> Result<Self, MaterialShaderError> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(MaterialShaderError::SourceLimit(format!(
                "material WGSL exceeds {MAX_SOURCE_BYTES} bytes"
            )));
        }
        if source.trim().is_empty() {
            return Err(MaterialShaderError::EmptySource);
        }
        if declaration.version() == 0 {
            return Err(MaterialShaderError::Interface(
                "material declaration version must be greater than zero".to_owned(),
            ));
        }
        validate_declaration_bounds(declaration)?;
        let mut counter = SourceByteCount(0);
        write_shader(&mut counter, declaration, source).map_err(|_| {
            MaterialShaderError::SourceLimit(format!(
                "assembled material WGSL exceeds {MAX_SOURCE_BYTES} bytes"
            ))
        })?;
        let mut assembled = String::with_capacity(counter.0);
        write_shader(&mut assembled, declaration, source).map_err(|_| {
            MaterialShaderError::SourceLimit("material WGSL assembly failed".to_owned())
        })?;
        let module = naga::front::wgsl::parse_str(&assembled)
            .map_err(|error| MaterialShaderError::Parse(parse_diagnostic(&error, &assembled)))?;
        Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .map_err(|error| {
                MaterialShaderError::Validation(validation_diagnostic(&error, &assembled))
            })?;
        validate_interface(declaration, &module)?;
        let interface_digest = interface_digest(declaration, &module)?;
        let digest = *blake3::hash(assembled.as_bytes()).as_bytes();
        Ok(Self {
            source: Arc::from(assembled),
            digest,
            interface_digest,
        })
    }

    /// Returns the complete stock, generated-declaration and user WGSL source.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the BLAKE3 digest of the exact complete source bytes.
    #[must_use]
    pub const fn digest(&self) -> [u8; 32] {
        self.digest
    }

    /// Returns the canonical observed interface and semantic-to-binding mapping.
    ///
    /// This includes resource names, groups, bindings, complete resource types,
    /// uniform member names/types/offsets/span, explicit stable field/texture IDs,
    /// the material fragment signature and the stock entry-point signatures.
    /// Function bodies, unrelated helper types and Naga arena handles are excluded.
    #[must_use]
    pub const fn interface_digest(&self) -> [u8; 32] {
        self.interface_digest
    }
}

/// A bounded, inspectable failure to compile a stock 2D material artifact.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MaterialShaderError {
    /// The supplied material source was empty or whitespace-only.
    #[error("material WGSL source is empty")]
    EmptySource,
    /// The supplied or assembled source exceeded its byte limit.
    #[error("material WGSL source limit: {0}")]
    SourceLimit(String),
    /// Naga could not parse or lower the complete source.
    #[error("material WGSL parse failed: {0}")]
    Parse(String),
    /// Naga rejected the compiled module.
    #[error("material WGSL validation failed: {0}")]
    Validation(String),
    /// The actual module did not match the declared stock material contract.
    #[error("material WGSL interface mismatch: {0}")]
    Interface(String),
}

struct SourceByteCount(usize);

impl fmt::Write for SourceByteCount {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.0 = self
            .0
            .checked_add(value.len())
            .filter(|size| *size <= MAX_SOURCE_BYTES)
            .ok_or(fmt::Error)?;
        Ok(())
    }
}

fn validate_declaration_bounds(
    declaration: &Material2dDeclaration,
) -> Result<(), MaterialShaderError> {
    let layout = declaration.parameters();
    if layout.fields().is_empty() || layout.fields().len() > MAX_MATERIAL_PARAMETERS {
        return Err(MaterialShaderError::Interface(
            "material uniform requires 1 through 32 fields".to_owned(),
        ));
    }
    if layout.byte_size() == 0 || layout.byte_size() > MAX_MATERIAL_UNIFORM_BYTES {
        return Err(MaterialShaderError::Interface(
            "material uniform requires 1 through 65536 bytes".to_owned(),
        ));
    }
    if declaration.textures().len() > MAX_MATERIAL_TEXTURES {
        return Err(MaterialShaderError::Interface(
            "material permits at most eight additional textures".to_owned(),
        ));
    }
    let mut bindings = 0_u16;
    for texture in declaration.textures() {
        if !(1..=MAX_MATERIAL_TEXTURES as u32).contains(&texture.binding()) {
            return Err(MaterialShaderError::Interface(
                "material texture bindings must be 1 through 8".to_owned(),
            ));
        }
        let bit = 1_u16 << texture.binding();
        if bindings & bit != 0 {
            return Err(MaterialShaderError::Interface(
                "material texture bindings must be unique".to_owned(),
            ));
        }
        bindings |= bit;
    }
    Ok(())
}

fn write_shader(
    writer: &mut impl fmt::Write,
    declaration: &Material2dDeclaration,
    source: &str,
) -> fmt::Result {
    writer.write_str(STOCK_FRAGMENT)?;
    writer.write_str("\nstruct MaterialParameters {\n")?;
    for field in declaration.parameters().fields() {
        writeln!(writer, "    {}: {},", field.name(), wgsl_type(field.kind()))?;
    }
    writeln!(
        writer,
        "}};\n\n@group({MATERIAL_BIND_GROUP}) @binding({MATERIAL_UNIFORM_BINDING})"
    )?;
    writer.write_str("var<uniform> material_parameters: MaterialParameters;\n")?;
    for texture in declaration.textures() {
        writeln!(
            writer,
            "\n@group({MATERIAL_BIND_GROUP}) @binding({})\nvar {}: texture_2d<f32>;",
            texture.binding(),
            texture.name()
        )?;
    }
    writer.write_str("\n")?;
    writer.write_str(source)?;
    writer.write_str("\n")
}

fn wgsl_type(kind: MaterialParameterKind) -> &'static str {
    match kind {
        MaterialParameterKind::F32 => "f32",
        MaterialParameterKind::I32 => "i32",
        MaterialParameterKind::U32 => "u32",
        MaterialParameterKind::Vec2 => "vec2<f32>",
        MaterialParameterKind::Vec3 => "vec3<f32>",
        MaterialParameterKind::Vec4 => "vec4<f32>",
    }
}

fn parameter_kind(inner: &TypeInner) -> Option<MaterialParameterKind> {
    use naga::{Scalar, VectorSize};

    match inner {
        TypeInner::Scalar(Scalar {
            kind: ScalarKind::Float,
            width: 4,
        }) => Some(MaterialParameterKind::F32),
        TypeInner::Scalar(Scalar {
            kind: ScalarKind::Sint,
            width: 4,
        }) => Some(MaterialParameterKind::I32),
        TypeInner::Scalar(Scalar {
            kind: ScalarKind::Uint,
            width: 4,
        }) => Some(MaterialParameterKind::U32),
        TypeInner::Vector {
            size,
            scalar:
                Scalar {
                    kind: ScalarKind::Float,
                    width: 4,
                },
        } => Some(match size {
            VectorSize::Bi => MaterialParameterKind::Vec2,
            VectorSize::Tri => MaterialParameterKind::Vec3,
            VectorSize::Quad => MaterialParameterKind::Vec4,
        }),
        _ => None,
    }
}

fn validate_interface(
    declaration: &Material2dDeclaration,
    module: &naga::Module,
) -> Result<(), MaterialShaderError> {
    if module.entry_points.len() != STOCK_ENTRY_POINTS.len()
        || STOCK_ENTRY_POINTS.iter().any(|(name, stage)| {
            !module
                .entry_points
                .iter()
                .any(|entry| entry.name == *name && entry.stage == *stage)
        })
    {
        return Err(MaterialShaderError::Interface(
            "only the stock vs_main, fs_main and fs_opaque entry points are permitted".to_owned(),
        ));
    }
    if !module.overrides.is_empty() {
        return Err(MaterialShaderError::Interface(
            "pipeline override constants are not declared material inputs".to_owned(),
        ));
    }
    let fragment = module
        .functions
        .iter()
        .find_map(|(_, function)| {
            (function.name.as_deref() == Some("material_fragment")).then_some(function)
        })
        .ok_or_else(|| {
            MaterialShaderError::Interface(
                "material_fragment must be an ordinary function".to_owned(),
            )
        })?;
    let expected_arguments = [
        MaterialParameterKind::Vec4,
        MaterialParameterKind::Vec4,
        MaterialParameterKind::Vec2,
    ];
    if fragment.arguments.len() != expected_arguments.len()
        || fragment
            .arguments
            .iter()
            .zip(expected_arguments)
            .any(|(argument, expected)| {
                argument.binding.is_some()
                    || parameter_kind(&module.types[argument.ty].inner) != Some(expected)
            })
        || fragment.result.as_ref().is_none_or(|result| {
            result.binding.is_some()
                || parameter_kind(&module.types[result.ty].inner)
                    != Some(MaterialParameterKind::Vec4)
        })
    {
        return Err(MaterialShaderError::Interface(
            "material_fragment must have signature (vec4<f32>, vec4<f32>, vec2<f32>) -> vec4<f32>"
                .to_owned(),
        ));
    }
    let mut expected_bindings = 0b111_u16;
    for texture in declaration.textures() {
        expected_bindings |= 1_u16 << (texture.binding() + 2);
    }
    let mut seen_bindings = 0_u16;
    for (_, variable) in module.global_variables.iter() {
        let Some(binding) = &variable.binding else {
            if variable.space != AddressSpace::Private {
                return Err(MaterialShaderError::Interface(
                    "only resource-free private globals may be added to the material".to_owned(),
                ));
            }
            continue;
        };
        let slot = match (binding.group, binding.binding) {
            (0, 0) => {
                validate_texture(module, variable, "sprite_texture")?;
                0
            }
            (0, 1) => {
                if variable.name.as_deref() != Some("sprite_sampler")
                    || variable.space != AddressSpace::Handle
                    || !matches!(
                        module.types[variable.ty].inner,
                        TypeInner::Sampler { comparison: false }
                    )
                {
                    return Err(MaterialShaderError::Interface(
                        "group 0 binding 1 must be the stock non-comparison sprite_sampler"
                            .to_owned(),
                    ));
                }
                1
            }
            (MATERIAL_BIND_GROUP, MATERIAL_UNIFORM_BINDING) => {
                if variable.name.as_deref() != Some("material_parameters")
                    || variable.space != AddressSpace::Uniform
                {
                    return Err(MaterialShaderError::Interface(
                        "group 1 binding 0 must be the material_parameters uniform".to_owned(),
                    ));
                }
                validate_uniform(declaration.parameters(), module, variable.ty)?;
                2
            }
            (MATERIAL_BIND_GROUP, texture_binding) => {
                let texture = declaration
                    .textures()
                    .iter()
                    .find(|texture| texture.binding() == texture_binding)
                    .ok_or_else(|| unauthorized_binding(binding))?;
                validate_texture(module, variable, texture.name())?;
                texture_binding + 2
            }
            _ => return Err(unauthorized_binding(binding)),
        };
        let bit = 1_u16 << slot;
        if seen_bindings & bit != 0 {
            return Err(MaterialShaderError::Interface(
                "compiled resource bindings must be unique even when unused".to_owned(),
            ));
        }
        seen_bindings |= bit;
    }
    if seen_bindings != expected_bindings {
        return Err(MaterialShaderError::Interface(
            "compiled shader must contain every stock and declared resource binding".to_owned(),
        ));
    }
    Ok(())
}

fn unauthorized_binding(binding: &naga::ResourceBinding) -> MaterialShaderError {
    MaterialShaderError::Interface(format!(
        "undeclared resource at group {} binding {}",
        binding.group, binding.binding
    ))
}

fn validate_texture(
    module: &naga::Module,
    variable: &naga::GlobalVariable,
    name: &str,
) -> Result<(), MaterialShaderError> {
    if variable.name.as_deref() != Some(name)
        || variable.space != AddressSpace::Handle
        || !is_sampled_texture(&module.types[variable.ty].inner)
    {
        return Err(MaterialShaderError::Interface(
            "texture binding must match its declared name and float-sampled, non-arrayed, non-multisampled 2D type"
                .to_owned(),
        ));
    }
    Ok(())
}

fn is_sampled_texture(inner: &TypeInner) -> bool {
    matches!(
        inner,
        TypeInner::Image {
            dim: ImageDimension::D2,
            arrayed: false,
            class: ImageClass::Sampled {
                kind: ScalarKind::Float,
                multi: false,
            },
        }
    )
}

fn validate_uniform(
    layout: &MaterialParameterLayout,
    module: &naga::Module,
    uniform: naga::Handle<naga::Type>,
) -> Result<(), MaterialShaderError> {
    let uniform = &module.types[uniform];
    let TypeInner::Struct { members, span } = &uniform.inner else {
        return Err(MaterialShaderError::Interface(
            "material_parameters must have struct type MaterialParameters".to_owned(),
        ));
    };
    if uniform.name.as_deref() != Some("MaterialParameters")
        || members.len() != layout.fields().len()
    {
        return Err(MaterialShaderError::Interface(
            "MaterialParameters name and member count must match the declaration".to_owned(),
        ));
    }
    if *span != layout.byte_size() {
        return Err(MaterialShaderError::Interface(format!(
            "MaterialParameters span {span} does not match declared uniform size {}",
            layout.byte_size()
        )));
    }
    for (index, (member, field)) in members.iter().zip(layout.fields()).enumerate() {
        if member.name.as_deref() != Some(field.name())
            || member.binding.is_some()
            || parameter_kind(&module.types[member.ty].inner) != Some(field.kind())
        {
            return Err(MaterialShaderError::Interface(format!(
                "MaterialParameters member {index} name or type does not match the declared field"
            )));
        }
        if member.offset != field.offset() {
            return Err(MaterialShaderError::Interface(format!(
                "MaterialParameters member {index} offset {} does not match declared offset {}",
                member.offset,
                field.offset()
            )));
        }
    }
    Ok(())
}

fn interface_digest(
    declaration: &Material2dDeclaration,
    module: &naga::Module,
) -> Result<[u8; 32], MaterialShaderError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"nara.material.shader-interface.v1\0");
    let mut resources: Vec<_> = module
        .global_variables
        .iter()
        .filter_map(|(_, variable)| {
            variable
                .binding
                .as_ref()
                .map(|binding| (binding.group, binding.binding, variable))
        })
        .collect();
    resources.sort_unstable_by_key(|(group, binding, _)| (*group, *binding));
    hasher.update(&(resources.len() as u32).to_le_bytes());
    for (group, binding, variable) in resources {
        hasher.update(&group.to_le_bytes());
        hasher.update(&binding.to_le_bytes());
        hash_string(&mut hasher, variable.name.as_deref().unwrap_or_default());
        match variable.space {
            AddressSpace::Handle => hasher.update(&[0]),
            AddressSpace::Uniform => hasher.update(&[1]),
            _ => {
                return Err(MaterialShaderError::Interface(
                    "unsupported resource address space".to_owned(),
                ));
            }
        };
        hash_type(&mut hasher, module, variable.ty)?;
        if group == MATERIAL_BIND_GROUP {
            if binding == MATERIAL_UNIFORM_BINDING {
                for field in declaration.parameters().fields() {
                    hash_string(&mut hasher, field.id());
                }
            } else {
                let texture = declaration
                    .textures()
                    .iter()
                    .find(|texture| texture.binding() == binding)
                    .ok_or_else(|| {
                        MaterialShaderError::Interface(
                            "missing declared texture mapping".to_owned(),
                        )
                    })?;
                hash_string(&mut hasher, texture.id());
            }
        }
    }
    let fragment = module
        .functions
        .iter()
        .find_map(|(_, function)| {
            (function.name.as_deref() == Some("material_fragment")).then_some(function)
        })
        .ok_or_else(|| {
            MaterialShaderError::Interface("missing material_fragment interface".to_owned())
        })?;
    hash_string(&mut hasher, "material_fragment");
    hash_function(&mut hasher, module, fragment)?;
    for (name, stage) in STOCK_ENTRY_POINTS {
        let entry = module
            .entry_points
            .iter()
            .find(|entry| entry.name == name && entry.stage == stage)
            .ok_or_else(|| {
                MaterialShaderError::Interface("missing stock entry-point interface".to_owned())
            })?;
        hash_string(&mut hasher, name);
        hasher.update(&[if stage == ShaderStage::Vertex { 0 } else { 1 }]);
        hash_function(&mut hasher, module, &entry.function)?;
    }
    Ok(*hasher.finalize().as_bytes())
}

fn hash_function(
    hasher: &mut blake3::Hasher,
    module: &naga::Module,
    function: &naga::Function,
) -> Result<(), MaterialShaderError> {
    hasher.update(&(function.arguments.len() as u32).to_le_bytes());
    for argument in &function.arguments {
        hash_binding(hasher, argument.binding.as_ref())?;
        hash_type(hasher, module, argument.ty)?;
    }
    if let Some(result) = &function.result {
        hasher.update(&[1]);
        hash_binding(hasher, result.binding.as_ref())?;
        hash_type(hasher, module, result.ty)?;
    } else {
        hasher.update(&[0]);
    }
    Ok(())
}

fn hash_type(
    hasher: &mut blake3::Hasher,
    module: &naga::Module,
    handle: naga::Handle<naga::Type>,
) -> Result<(), MaterialShaderError> {
    let ty = &module.types[handle];
    if let Some(kind) = parameter_kind(&ty.inner) {
        hasher.update(&[0]);
        hash_string(hasher, wgsl_type(kind));
        return Ok(());
    }
    match &ty.inner {
        TypeInner::Struct { members, span } => {
            hasher.update(&[1]);
            hash_string(hasher, ty.name.as_deref().unwrap_or_default());
            hasher.update(&span.to_le_bytes());
            hasher.update(&(members.len() as u32).to_le_bytes());
            for member in members {
                hash_string(hasher, member.name.as_deref().unwrap_or_default());
                hasher.update(&member.offset.to_le_bytes());
                hash_binding(hasher, member.binding.as_ref())?;
                hash_type(hasher, module, member.ty)?;
            }
        }
        inner if is_sampled_texture(inner) => {
            hasher.update(&[2]);
            hash_string(hasher, "texture_2d<f32>");
        }
        TypeInner::Sampler { comparison: false } => {
            hasher.update(&[3]);
            hash_string(hasher, "sampler");
        }
        _ => {
            return Err(MaterialShaderError::Interface(
                "unsupported type in the stock shader interface".to_owned(),
            ));
        }
    }
    Ok(())
}

fn hash_binding(
    hasher: &mut blake3::Hasher,
    binding: Option<&naga::Binding>,
) -> Result<(), MaterialShaderError> {
    use naga::{Binding, BuiltIn, Interpolation, Sampling};

    match binding {
        None => {
            hasher.update(&[0]);
        }
        Some(Binding::BuiltIn(BuiltIn::VertexIndex)) => {
            hasher.update(&[1]);
        }
        Some(Binding::BuiltIn(BuiltIn::Position { invariant })) => {
            hasher.update(&[2, u8::from(*invariant)]);
        }
        Some(Binding::Location {
            location,
            interpolation,
            sampling,
            blend_src,
            per_primitive,
        }) => {
            hasher.update(&[3]);
            hasher.update(&location.to_le_bytes());
            hasher.update(&[match interpolation {
                None => 0,
                Some(Interpolation::Perspective) => 1,
                Some(Interpolation::Linear) => 2,
                Some(Interpolation::Flat) => 3,
                Some(Interpolation::PerVertex) => 4,
            }]);
            hasher.update(&[match sampling {
                None => 0,
                Some(Sampling::Center) => 1,
                Some(Sampling::Centroid) => 2,
                Some(Sampling::Sample) => 3,
                Some(Sampling::First) => 4,
                Some(Sampling::Either) => 5,
            }]);
            if let Some(blend_src) = blend_src {
                hasher.update(&[1]);
                hasher.update(&blend_src.to_le_bytes());
            } else {
                hasher.update(&[0]);
            }
            hasher.update(&[u8::from(*per_primitive)]);
        }
        Some(_) => {
            return Err(MaterialShaderError::Interface(
                "unsupported builtin in the stock shader interface".to_owned(),
            ));
        }
    }
    Ok(())
}

fn hash_string(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn parse_diagnostic(error: &naga::front::wgsl::ParseError, source: &str) -> String {
    if source.len() <= MAX_ANNOTATED_SOURCE_BYTES
        && error.labels().len() <= MAX_DIAGNOSTIC_SPANS
        && error.notes().len() <= MAX_DIAGNOSTIC_SPANS
    {
        bounded_diagnostic(error.emit_to_string(source))
    } else {
        compiler_summary(error, error.location(source))
    }
}

fn validation_diagnostic(
    error: &naga::WithSpan<naga::valid::ValidationError>,
    source: &str,
) -> String {
    if source.len() <= MAX_ANNOTATED_SOURCE_BYTES && error.spans().len() <= MAX_DIAGNOSTIC_SPANS {
        bounded_diagnostic(error.emit_to_string(source))
    } else {
        compiler_summary(error, error.location(source))
    }
}

fn compiler_summary(error: &dyn Error, location: Option<naga::SourceLocation>) -> String {
    let mut summary = bounded_diagnostic(error.to_string());
    if let Some(location) = location {
        let _ = write!(
            summary,
            "\nWGSL line {}, column {}",
            location.line_number, location.line_position
        );
    }
    let mut cause = error.source();
    for _ in 0..MAX_DIAGNOSTIC_SPANS {
        let Some(current) = cause else {
            break;
        };
        if summary.len() >= MAX_DIAGNOSTIC_BYTES {
            break;
        }
        summary.push_str("\ncaused by: ");
        summary.push_str(&bounded_diagnostic(current.to_string()));
        cause = current.source();
    }
    bounded_diagnostic(summary)
}

fn bounded_diagnostic(mut diagnostic: String) -> String {
    const TRUNCATED: &str = "\n[diagnostic truncated]";
    if diagnostic.len() > MAX_DIAGNOSTIC_BYTES {
        let mut end = MAX_DIAGNOSTIC_BYTES - TRUNCATED.len();
        while !diagnostic.is_char_boundary(end) {
            end -= 1;
        }
        diagnostic.truncate(end);
        diagnostic.push_str(TRUNCATED);
    }
    diagnostic
}

#[cfg(test)]
mod tests;
