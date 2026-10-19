use std::collections::{BTreeMap, BTreeSet};

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote, quote_spanned};
use syn::{
    Attribute, Data, DeriveInput, Error, Expr, Field, Fields, Ident, Lit, LitInt, LitStr, Path,
    Result, Type, parse_macro_input, spanned::Spanned,
};

/// Derives a persistent schema and its sealed field codecs.
///
/// Component-level `#[nara(authoring_initial)]` opts into data-only creation. Every field
/// then requires `#[nara(initial = <literal>)]`; supported values are boolean, string,
/// integer and finite float literals, including negative numeric literals. Rust checks
/// each literal against its field type. Expressions, callbacks and aggregate values are
/// intentionally unsupported. Creation metadata never supplies missing decode fields.
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
    authoring_initial: Option<Span>,
}

#[derive(Default)]
struct FieldArgs {
    id: Option<LitStr>,
    aliases: Vec<LitStr>,
    capabilities: Option<Vec<Capability>>,
    initial: Option<Expr>,
    range: Option<NumericRange>,
}

struct PersistentField {
    ident: Ident,
    ty: Type,
    id: LitStr,
    aliases: Vec<LitStr>,
    capabilities: Vec<Capability>,
    initial: Option<Expr>,
    range: Option<NumericRange>,
}

#[derive(Default)]
struct NumericRange {
    min: Option<(bool, Expr)>,
    max: Option<(bool, Expr)>,
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

    for field in &persistent_fields {
        match (args.authoring_initial, &field.initial) {
            (Some(_), None) => {
                return Err(Error::new(
                    field.ident.span(),
                    "authoring_initial requires an explicit `initial` literal on every field",
                ));
            }
            (None, Some(initial)) => {
                return Err(Error::new_spanned(
                    initial,
                    "field `initial` requires component `authoring_initial`",
                ));
            }
            _ => {}
        }
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
    let authoring_initial = if args.authoring_initial.is_some() {
        let values = persistent_fields.iter().map(|field| {
            let id = &field.id;
            let ty = &field.ty;
            let initial = field.initial.as_ref().expect("creation fields were validated");
            let value = match initial {
                Expr::Lit(literal) if matches!(literal.lit, Lit::Str(_)) => {
                    quote!(::std::string::String::from(#initial))
                }
                _ if matches!(initial_scalar_literal(initial), Ok(literal) if is_float_literal(literal)) => {
                    quote! {
                        const {
                            let initial_value: #ty = #initial;
                            assert!(
                                initial_value.is_finite(),
                                "authoring initial float must be finite for its field type"
                            );
                            initial_value
                        }
                    }
                }
                _ => quote!(#initial),
            };
            quote_spanned!(initial.span()=> (#id, {
                let initial_value: #ty = #value;
                #support::encode_persistent_field(&initial_value)
                    .expect("PersistentComponent validates finite scalar creation literals")
            }))
        });
        let value = if persistent_fields.is_empty() {
            quote!(#support::ComponentValue::map(::core::iter::empty::<(
                &'static str, #support::ComponentValue,
            )>()))
        } else {
            quote!(#support::ComponentValue::map([#(#values),*]))
        };
        quote!(.with_authoring_initial_value(#value))
    } else {
        TokenStream2::new()
    };
    let component_encoder = if persistent_fields.is_empty() {
        quote!(#support::ComponentValue::map(::core::iter::empty::<(
            &'static str,
            #support::ComponentValue,
        )>()))
    } else {
        let field_encoders = persistent_fields.iter().map(|field| {
            let ident = &field.ident;
            let id = &field.id;
            quote!((#id, #support::encode_persistent_field(&self.#ident)?))
        });
        quote!(#support::ComponentValue::map([#(#field_encoders),*]))
    };
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
                #authoring_initial
            }

            fn __decode_persistent_component(
                value: &#support::ComponentValue,
            ) -> Result<Self, #support::ComponentCodecError> {
                Ok(Self { #(#field_decoders),* })
            }

            fn __encode_persistent_component(
                &self,
            ) -> Result<#support::ComponentValue, #support::ComponentCodecError> {
                Ok(#component_encoder)
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
            } else if meta.path.is_ident("authoring_initial") {
                set_once(
                    &mut args.authoring_initial,
                    meta.path.span(),
                    meta.path.span(),
                    "authoring_initial",
                )
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
            } else if meta.path.is_ident("initial") {
                set_once(
                    &mut args.initial,
                    meta.value()?.parse()?,
                    meta.path.span(),
                    "initial",
                )
            } else if meta.path.is_ident("range") {
                let range = parse_numeric_range(meta)?;
                set_once(&mut args.range, range, attribute.span(), "range")
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
    if let Some(initial) = &args.initial {
        validate_initial_literal(initial)?;
    }
    Ok(PersistentField {
        ident,
        ty: field.ty,
        id,
        aliases: args.aliases,
        capabilities,
        initial: args.initial,
        range: args.range,
    })
}

fn parse_numeric_range(meta: syn::meta::ParseNestedMeta<'_>) -> Result<NumericRange> {
    let mut range = NumericRange::default();
    meta.parse_nested_meta(|bound| {
        let (side, inclusive) = if bound.path.is_ident("min_inclusive") {
            (&mut range.min, true)
        } else if bound.path.is_ident("min_exclusive") {
            (&mut range.min, false)
        } else if bound.path.is_ident("max_inclusive") {
            (&mut range.max, true)
        } else if bound.path.is_ident("max_exclusive") {
            (&mut range.max, false)
        } else {
            return Err(bound
                .error("expected min_inclusive, min_exclusive, max_inclusive or max_exclusive"));
        };
        if side.is_some() {
            return Err(bound.error("a range may declare each endpoint only once"));
        }
        let value: Expr = bound.value()?.parse()?;
        if !matches!(
            initial_scalar_literal(&value),
            Ok(Lit::Int(_) | Lit::Float(_))
        ) {
            return Err(Error::new_spanned(
                value,
                "range bounds must be numeric literals or negative numeric literals",
            ));
        }
        *side = Some((inclusive, value));
        Ok(())
    })?;
    if range.min.is_none() && range.max.is_none() {
        return Err(meta.error("range requires at least one bound"));
    }
    Ok(range)
}

fn initial_scalar_literal(initial: &Expr) -> Result<&Lit> {
    let literal = match initial {
        Expr::Lit(literal) => &literal.lit,
        Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Neg(_)) => match unary.expr.as_ref() {
            Expr::Lit(literal) if matches!(literal.lit, Lit::Int(_) | Lit::Float(_)) => {
                &literal.lit
            }
            _ => {
                return Err(Error::new_spanned(
                    initial,
                    "initial must be a scalar literal or negative numeric literal",
                ));
            }
        },
        _ => {
            return Err(Error::new_spanned(
                initial,
                "initial must be a scalar literal or negative numeric literal",
            ));
        }
    };
    Ok(literal)
}

fn validate_initial_literal(initial: &Expr) -> Result<()> {
    let literal = initial_scalar_literal(initial)?;
    match literal {
        Lit::Bool(_) | Lit::Str(_) | Lit::Int(_) | Lit::Float(_) => Ok(()),
        _ => Err(Error::new_spanned(
            initial,
            "initial must be a scalar literal or negative numeric literal",
        )),
    }
}

fn is_float_literal(literal: &Lit) -> bool {
    match literal {
        Lit::Float(_) => true,
        // Rust permits integer-shaped float literals; syn preserves their lexical shape.
        Lit::Int(literal) => matches!(literal.suffix(), "f32" | "f64"),
        _ => false,
    }
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

fn field_schema(field: &PersistentField, support: &TokenStream2) -> TokenStream2 {
    let ty = &field.ty;
    let id = &field.id;
    let primary_alias = &field.aliases[0];
    let additional_aliases = alias_extension(&field.aliases[1..]);
    let capability_tokens = field
        .capabilities
        .iter()
        .map(|capability| capability.tokens(support));
    let interval = field.range.as_ref().map(|range| {
        let bound_tokens = |bound: &Option<(bool, Expr)>| match bound {
            None => quote!(::core::option::Option::None),
            Some((inclusive, value)) => {
                let variant = if *inclusive { format_ident!("Included") } else { format_ident!("Excluded") };
                let checked = if matches!(initial_scalar_literal(value), Ok(literal) if is_float_literal(literal)) {
                    quote! { const {
                        let bound: #ty = #value;
                        assert!(bound.is_finite(), "range bounds must be finite for their field type");
                        bound
                    } }
                } else { quote!(#value) };
                quote!(::core::option::Option::Some(#support::ComponentNumericBound::#variant({
                    let bound: #ty = #checked;
                    bound
                })))
            }
        };
        let min = bound_tokens(&range.min);
        let max = bound_tokens(&range.max);
        quote! { .with_numeric_interval(
            <#ty as #support::PersistentNumericField>::numeric_interval(#min, #max)
                .expect("PersistentComponent range literals are finite")
        ) }
    });
    quote! {
        #support::ComponentFieldSchema::required(
            #support::ComponentFieldId::new(#id),
            #primary_alias,
            #support::ComponentFieldPath::from_fields([#id]),
            <#ty as #support::PersistentFieldCodec>::value_shape(),
        )
        #additional_aliases
        .with_capabilities([#(#capability_tokens),*])
        #interval
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
