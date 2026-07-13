use std::collections::{BTreeMap, BTreeSet};

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
    Attribute, Data, DeriveInput, Error, Field, Fields, Ident, LitInt, LitStr, Path, Result, Type,
    parse_macro_input, spanned::Spanned,
};

#[proc_macro_derive(PersistentComponent, attributes(nara))]
pub fn derive_persistent_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_persistent_component(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Capability {
    Scene,
    Inspect,
    Edit,
    AssetRef,
    EntityRef,
}

impl Capability {
    fn parse(ident: &Ident) -> Result<Self> {
        match ident.to_string().as_str() {
            "scene" => Ok(Self::Scene),
            "inspect" => Ok(Self::Inspect),
            "edit" => Ok(Self::Edit),
            "asset_ref" => Ok(Self::AssetRef),
            "entity_ref" => Ok(Self::EntityRef),
            _ => Err(Error::new(
                ident.span(),
                "unsupported canonical-v1 component capability",
            )),
        }
    }

    fn tokens(self, support: &TokenStream2) -> TokenStream2 {
        let variant = match self {
            Self::Scene => format_ident!("Scene"),
            Self::Inspect => format_ident!("Inspect"),
            Self::Edit => format_ident!("Edit"),
            Self::AssetRef => format_ident!("AssetRef"),
            Self::EntityRef => format_ident!("EntityRef"),
        };
        quote!(#support::ComponentCapability::#variant)
    }

    fn is_component_capability(self) -> bool {
        matches!(self, Self::Scene | Self::Inspect | Self::Edit)
    }
}

#[derive(Default)]
struct ComponentArgs {
    id: Option<LitStr>,
    version: Option<LitInt>,
    aliases: Vec<LitStr>,
    component_capabilities: Option<Vec<Capability>>,
    field_capabilities: Option<Vec<Capability>>,
    tombstones: Vec<LitStr>,
    crate_path: Option<Path>,
}

#[derive(Default)]
struct FieldArgs {
    id: Option<LitStr>,
    aliases: Vec<LitStr>,
    capabilities: Option<Vec<Capability>>,
}

struct PersistentField {
    ident: Ident,
    ty: Type,
    id: LitStr,
    aliases: Vec<LitStr>,
    capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersistentFieldKind {
    I64,
    U64,
    Vec2,
    EntityReference,
}

fn expand_persistent_component(input: DeriveInput) -> Result<TokenStream2> {
    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        return Err(Error::new_spanned(
            input.generics,
            "PersistentComponent supports only non-generic structs",
        ));
    }
    let fields = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => fields.named,
            other => {
                return Err(Error::new_spanned(
                    other,
                    "PersistentComponent requires a struct with named fields",
                ));
            }
        },
        Data::Enum(_) | Data::Union(_) => {
            return Err(Error::new(
                input.ident.span(),
                "PersistentComponent can only be derived for structs",
            ));
        }
    };

    let args = parse_component_args(&input.attrs)?;
    let component_id = require(args.id, input.ident.span(), "missing component `id`")?;
    let version = require(
        args.version,
        input.ident.span(),
        "missing component `version`",
    )?;
    let version_value = version.base10_parse::<u32>()?;
    if version_value == 0 {
        return Err(Error::new(
            version.span(),
            "component version must be non-zero",
        ));
    }
    if args.aliases.is_empty() {
        return Err(Error::new(input.ident.span(), "missing component `alias`"));
    }
    let component_capabilities = require(
        args.component_capabilities,
        input.ident.span(),
        "missing `component_capabilities(...)`",
    )?;
    let default_field_capabilities = require(
        args.field_capabilities,
        input.ident.span(),
        "missing `field_capabilities(...)`",
    )?;
    validate_component_capabilities(&component_capabilities, input.ident.span())?;
    validate_field_subset(
        &default_field_capabilities,
        &component_capabilities,
        input.ident.span(),
    )?;

    let mut persistent_fields = Vec::with_capacity(fields.len());
    let mut field_ids = BTreeMap::new();
    for field in fields {
        let persistent = parse_field(field, &default_field_capabilities, &component_capabilities)?;
        if let Some(previous_span) = field_ids.insert(persistent.id.value(), persistent.id.span()) {
            let mut error = Error::new(persistent.id.span(), "duplicate persistent field ID");
            error.combine(Error::new(previous_span, "first declared here"));
            return Err(error);
        }
        persistent_fields.push(persistent);
    }

    let mut tombstones = BTreeSet::new();
    for tombstone in &args.tombstones {
        if !tombstones.insert(tombstone.value()) {
            return Err(Error::new(tombstone.span(), "duplicate field tombstone"));
        }
        if field_ids.contains_key(&tombstone.value()) {
            return Err(Error::new(
                tombstone.span(),
                "field tombstone cannot reactivate an active field ID",
            ));
        }
    }

    let support = support_path(args.crate_path)?;
    let name = input.ident;
    let primary_alias = &args.aliases[0];
    let additional_aliases = alias_extension(&args.aliases[1..]);
    let component_capability_tokens = component_capabilities
        .iter()
        .map(|capability| capability.tokens(&support));
    let tombstone_values = args.tombstones.iter();
    let field_schemas = persistent_fields
        .iter()
        .map(|field| field_schema(field, &support));
    let field_encoders = persistent_fields.iter().map(|field| {
        let ident = &field.ident;
        let id = &field.id;
        quote!((#id, #support::encode_persistent_field(&self.#ident)?))
    });
    let field_decoders = persistent_fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        let id = &field.id;
        quote!(#ident: #support::decode_persistent_field::<#ty>(value, #id)?)
    });

    Ok(quote! {
        impl #support::PersistentComponentProvider for #name {
            fn persistent_component_schema() -> #support::ComponentSchema {
                #support::ComponentSchema::new(
                    #support::ComponentTypeId::new(#component_id),
                    #primary_alias,
                    #support::ComponentSchemaVersion::new(#version_value)
                        .expect("PersistentComponent validates non-zero schema versions"),
                )
                #additional_aliases
                .with_capabilities([#(#component_capability_tokens),*])
                .with_fields([#(#field_schemas),*])
                .with_field_tombstones([
                    #(#support::ComponentFieldId::new(#tombstone_values)),*
                ])
            }

            fn __decode_persistent_component(
                value: &#support::ComponentValue,
            ) -> Result<Self, #support::ComponentCodecError> {
                Ok(Self { #(#field_decoders),* })
            }

            fn __encode_persistent_component(
                &self,
            ) -> Result<#support::ComponentValue, #support::ComponentCodecError> {
                Ok(#support::ComponentValue::map([#(#field_encoders),*]))
            }
        }
    })
}

fn parse_component_args(attributes: &[Attribute]) -> Result<ComponentArgs> {
    let mut args = ComponentArgs::default();
    for attribute in attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("nara"))
    {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("id") {
                set_once(&mut args.id, meta.value()?.parse()?, meta.path.span(), "id")
            } else if meta.path.is_ident("version") {
                set_once(
                    &mut args.version,
                    meta.value()?.parse()?,
                    meta.path.span(),
                    "version",
                )
            } else if meta.path.is_ident("alias") {
                args.aliases.push(meta.value()?.parse()?);
                Ok(())
            } else if meta.path.is_ident("component_capabilities") {
                let capabilities = parse_capabilities(meta)?;
                set_once(
                    &mut args.component_capabilities,
                    capabilities,
                    attribute.span(),
                    "component_capabilities",
                )
            } else if meta.path.is_ident("field_capabilities") {
                let capabilities = parse_capabilities(meta)?;
                set_once(
                    &mut args.field_capabilities,
                    capabilities,
                    attribute.span(),
                    "field_capabilities",
                )
            } else if meta.path.is_ident("tombstone") {
                args.tombstones.push(meta.value()?.parse()?);
                Ok(())
            } else if meta.path.is_ident("crate") {
                let literal: LitStr = meta.value()?.parse()?;
                let path = literal.parse()?;
                set_once(&mut args.crate_path, path, literal.span(), "crate")
            } else {
                Err(meta.error("unsupported PersistentComponent attribute"))
            }
        })?;
    }
    Ok(args)
}

fn parse_field(
    field: Field,
    default_capabilities: &[Capability],
    component_capabilities: &[Capability],
) -> Result<PersistentField> {
    if let Some(attribute) = field
        .attrs
        .iter()
        .find(|attribute| attribute.path().is_ident("cfg") || attribute.path().is_ident("cfg_attr"))
    {
        return Err(Error::new_spanned(
            attribute,
            "configuration-dependent persistent fields are not supported",
        ));
    }
    let kind = classify_field_type(&field.ty)?;
    let ident = field
        .ident
        .ok_or_else(|| Error::new_spanned(&field.ty, "persistent field must be named"))?;
    let mut args = FieldArgs::default();
    for attribute in field
        .attrs
        .iter()
        .filter(|attribute| attribute.path().is_ident("nara"))
    {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("id") {
                set_once(&mut args.id, meta.value()?.parse()?, meta.path.span(), "id")
            } else if meta.path.is_ident("alias") {
                args.aliases.push(meta.value()?.parse()?);
                Ok(())
            } else if meta.path.is_ident("capabilities") {
                let capabilities = parse_capabilities(meta)?;
                set_once(
                    &mut args.capabilities,
                    capabilities,
                    attribute.span(),
                    "capabilities",
                )
            } else {
                Err(meta.error("unsupported persistent field attribute"))
            }
        })?;
    }
    let id = require(args.id, ident.span(), "missing persistent field `id`")?;
    if args.aliases.is_empty() {
        return Err(Error::new(ident.span(), "missing persistent field `alias`"));
    }
    let capabilities = args
        .capabilities
        .unwrap_or_else(|| default_capabilities.to_vec());
    validate_field_subset(&capabilities, component_capabilities, ident.span())?;
    validate_type_capabilities(&field.ty, kind, &capabilities)?;
    Ok(PersistentField {
        ident,
        ty: field.ty,
        id,
        aliases: args.aliases,
        capabilities,
    })
}

fn parse_capabilities(meta: syn::meta::ParseNestedMeta<'_>) -> Result<Vec<Capability>> {
    let mut capabilities = BTreeSet::new();
    meta.parse_nested_meta(|nested| {
        let Some(ident) = nested.path.get_ident() else {
            return Err(nested.error("capability must be a single identifier"));
        };
        let capability = Capability::parse(ident)?;
        if !capabilities.insert(capability) {
            return Err(nested.error("duplicate component capability"));
        }
        Ok(())
    })?;
    Ok(capabilities.into_iter().collect())
}

fn validate_component_capabilities(capabilities: &[Capability], span: Span) -> Result<()> {
    if capabilities
        .iter()
        .any(|capability| !capability.is_component_capability())
    {
        return Err(Error::new(
            span,
            "asset_ref and entity_ref are field-only capabilities",
        ));
    }
    Ok(())
}

fn validate_field_subset(field: &[Capability], component: &[Capability], span: Span) -> Result<()> {
    if let Some(capability) = field
        .iter()
        .find(|capability| capability.is_component_capability() && !component.contains(capability))
    {
        return Err(Error::new(
            span,
            format!("field capability {capability:?} exceeds component capability"),
        ));
    }
    Ok(())
}

fn validate_type_capabilities(
    ty: &Type,
    kind: PersistentFieldKind,
    capabilities: &[Capability],
) -> Result<()> {
    let has_entity_ref = capabilities.contains(&Capability::EntityRef);
    if matches!(kind, PersistentFieldKind::EntityReference) != has_entity_ref {
        return Err(Error::new_spanned(
            ty,
            "EntityReference fields must declare entity_ref, and other fields must not",
        ));
    }
    if capabilities.contains(&Capability::AssetRef) {
        return Err(Error::new_spanned(
            ty,
            "asset_ref fields are not supported by the current authoring tracer",
        ));
    }
    Ok(())
}

fn classify_field_type(ty: &Type) -> Result<PersistentFieldKind> {
    let Type::Path(path) = ty else {
        return Err(Error::new_spanned(ty, "unsupported persistent field type"));
    };
    if path.qself.is_some() {
        return Err(Error::new_spanned(ty, "unsupported persistent field type"));
    }
    let segment = path
        .path
        .segments
        .last()
        .ok_or_else(|| Error::new_spanned(ty, "unsupported persistent field type"))?;
    if !matches!(segment.arguments, syn::PathArguments::None) {
        return Err(Error::new_spanned(
            ty,
            "unsupported persistent field type; collection and generic fields are deferred",
        ));
    }
    match segment.ident.to_string().as_str() {
        "i64" => Ok(PersistentFieldKind::I64),
        "u64" => Ok(PersistentFieldKind::U64),
        "Vec2" => Ok(PersistentFieldKind::Vec2),
        "EntityReference" => Ok(PersistentFieldKind::EntityReference),
        _ => Err(Error::new_spanned(
            ty,
            "unsupported persistent field type; use an admitted scalar, Vec2, or EntityReference",
        )),
    }
}

fn field_schema(field: &PersistentField, support: &TokenStream2) -> TokenStream2 {
    let ty = &field.ty;
    let id = &field.id;
    let primary_alias = &field.aliases[0];
    let additional_aliases = alias_extension(&field.aliases[1..]);
    let capability_tokens = field
        .capabilities
        .iter()
        .map(|capability| capability.tokens(support));
    quote! {
        #support::ComponentFieldSchema::required(
            #support::ComponentFieldId::new(#id),
            #primary_alias,
            #support::ComponentFieldPath::from_fields([#id]),
            <#ty as #support::PersistentFieldCodec>::VALUE_KIND,
        )
        #additional_aliases
        .with_capabilities([#(#capability_tokens),*])
    }
}

fn alias_extension(aliases: &[LitStr]) -> TokenStream2 {
    if aliases.is_empty() {
        TokenStream2::new()
    } else {
        quote!(.with_aliases([#(#aliases),*]))
    }
}

fn support_path(explicit: Option<Path>) -> Result<TokenStream2> {
    if let Some(path) = explicit {
        return Ok(quote!(#path::__macro_support));
    }
    for package in ["nara_reflect", "nara"] {
        match crate_name(package) {
            Ok(FoundCrate::Itself) => return Ok(quote!(crate::__macro_support)),
            Ok(FoundCrate::Name(name)) => {
                let ident = Ident::new(&name, Span::call_site());
                return Ok(quote!(::#ident::__macro_support));
            }
            Err(_) => {}
        }
    }
    Err(Error::new(
        Span::call_site(),
        "PersistentComponent requires a direct nara or nara_reflect dependency; use #[nara(crate = \"path\")] to override it",
    ))
}

fn set_once<T>(slot: &mut Option<T>, value: T, span: Span, name: &str) -> Result<()> {
    if slot.is_some() {
        return Err(Error::new(span, format!("duplicate `{name}` attribute")));
    }
    *slot = Some(value);
    Ok(())
}

fn require<T>(value: Option<T>, span: Span, message: &str) -> Result<T> {
    value.ok_or_else(|| Error::new(span, message))
}
