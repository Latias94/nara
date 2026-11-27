use std::collections::{BTreeMap, BTreeSet};

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote, quote_spanned};
use syn::{
    Attribute, Data, DeriveInput, Error, Expr, ExprRange, Field, Fields, Ident, Lit, LitInt,
    LitStr, Path, Result, Token, Type, ext::IdentExt, parse_macro_input, spanned::Spanned,
};

mod metadata;

use metadata::FieldMetadata;

/// Derives a native component schema and its codecs.
///
/// A component declares `#[schema(id = "game.Health", version = 1)]`. Fields use their
/// Rust names unless given a stable `id`. `#[schema(default)]` on the type uses its Rust
/// `Default` for omitted fields. Field defaults can override it individually.
#[proc_macro_derive(Schema, attributes(schema))]
pub fn derive_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_schema(input)
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
    group: Option<LitStr>,
    use_default: Option<Span>,
}

#[derive(Default)]
struct FieldArgs {
    id: Option<LitStr>,
    aliases: Vec<LitStr>,
    capabilities: Option<Vec<Capability>>,
    initial: Option<Expr>,
    range: Option<NumericRange>,
    asset_demand: Option<(AssetDemand, Span)>,
    metadata: FieldMetadata,
}

struct PersistentField {
    ident: Ident,
    ty: Type,
    id: LitStr,
    aliases: Vec<LitStr>,
    capabilities: Vec<Capability>,
    initial: Option<Expr>,
    range: Option<NumericRange>,
    asset_demand: Option<(AssetDemand, Span)>,
    metadata: FieldMetadata,
}

#[derive(Clone, Copy)]
enum AssetDemand {
    Immediate,
    Deferred,
}

impl AssetDemand {
    fn parse(ident: &Ident) -> Result<Self> {
        match ident.to_string().as_str() {
            "immediate" => Ok(Self::Immediate),
            "deferred" => Ok(Self::Deferred),
            _ => Err(Error::new(
                ident.span(),
                "asset_demand must be `immediate` or `deferred`",
            )),
        }
    }

    fn tokens(self, support: &TokenStream2) -> TokenStream2 {
        let variant = match self {
            Self::Immediate => format_ident!("Immediate"),
            Self::Deferred => format_ident!("Deferred"),
        };
        quote!(#support::AssetReferenceDemand::#variant)
    }
}

#[derive(Default)]
struct NumericRange {
    min: Option<(bool, Expr)>,
    max: Option<(bool, Expr)>,
}

fn expand_schema(input: DeriveInput) -> Result<TokenStream2> {
    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        return Err(Error::new_spanned(
            input.generics,
            "Schema supports only non-generic structs",
        ));
    }
    let (fields, is_unit) = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => (fields.named, false),
            Fields::Unit => (Default::default(), true),
            other => {
                return Err(Error::new_spanned(
                    other,
                    "Schema requires a struct with named fields or a unit struct",
                ));
            }
        },
        Data::Enum(_) | Data::Union(_) => {
            return Err(Error::new(
                input.ident.span(),
                "Schema can only be derived for component structs",
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
    let component_capabilities = args
        .component_capabilities
        .unwrap_or_else(|| vec![Capability::Scene, Capability::Inspect, Capability::Edit]);
    let default_field_capabilities = args
        .field_capabilities
        .unwrap_or_else(|| component_capabilities.clone());
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
        if persistent.metadata.skip.is_some() {
            persistent_fields.push(persistent);
            continue;
        }
        if let Some(previous_span) = field_ids.insert(persistent.id.value(), persistent.id.span()) {
            let mut error = Error::new(persistent.id.span(), "duplicate persistent field ID");
            error.combine(Error::new(previous_span, "first declared here"));
            return Err(error);
        }
        persistent_fields.push(persistent);
    }

    for field in &persistent_fields {
        if field.metadata.skip.is_some() {
            continue;
        }
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
    let (skipped, persistent_fields): (Vec<_>, Vec<_>) = persistent_fields
        .into_iter()
        .partition(|field| field.metadata.skip.is_some());
    let finite_literal_checks = finite_literal_checks(&name, &persistent_fields);
    let asset_demand_checks = asset_demand_checks(&name, &persistent_fields, &support);
    let (primary_alias, aliases) = args.aliases.split_first().map_or_else(
        || (LitStr::new(&name.unraw().to_string(), name.span()), &[][..]),
        |(first, rest)| (first.clone(), rest),
    );
    let additional_aliases = alias_extension(aliases);
    let group = args.group.map(|group| quote!(.with_group(#group)));
    let component_capability_tokens = component_capabilities
        .iter()
        .map(|capability| capability.tokens(&support));
    let tombstone_values = args.tombstones.iter();
    let use_default = args.use_default.is_some()
        && persistent_fields
            .iter()
            .any(|field| field.metadata.default.is_none());
    let component_defaults = use_default
        .then(|| quote!(let schema_defaults = <Self as ::core::default::Default>::default();));
    let field_schemas = persistent_fields
        .iter()
        .map(|field| field_schema(field, &support, use_default));
    let authoring_initial = if args.authoring_initial.is_some() {
        let values = persistent_fields.iter().map(|field| {
            let id = &field.id;
            let ty = &field.ty;
            let initial = field
                .initial
                .as_ref()
                .expect("creation fields were validated");
            let value = match initial {
                Expr::Lit(literal) if matches!(literal.lit, Lit::Str(_)) => {
                    quote!(::std::string::String::from(#initial))
                }
                _ => quote!(#initial),
            };
            quote_spanned!(initial.span()=> (#id, {
                let initial_value: #ty = #value;
                #support::encode_persistent_field(&initial_value)?
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
    let field_decoders = persistent_fields.iter().enumerate().map(|(index, field)| {
        let ident = &field.ident;
        let ty = &field.ty;
        quote!(#ident: #support::decode_persistent_field::<#ty>(value, &schema.fields()[#index])?)
    });
    let skipped_decoders = skipped
        .iter()
        .map(|field| {
            let ident = &field.ident;
            let ty = &field.ty;
            let value = metadata::rust_default(field.metadata.default.as_ref(), ty);
            quote!(#ident: #value)
        })
        .collect::<Vec<_>>();
    let component_decoder = if is_unit {
        quote!(Self)
    } else {
        quote!(Self { #(#field_decoders,)* #(#skipped_decoders,)* })
    };
    let field_access = field_accessors(&persistent_fields, &support);
    let postcard = postcard_methods(&persistent_fields, &skipped_decoders, is_unit, &support);

    Ok(quote! {
        #finite_literal_checks
        #asset_demand_checks

        impl #support::Schema for #name {
            const COMPONENT_ID: &'static str = #component_id;

            fn type_schema() -> ::core::result::Result<#support::TypeSchema, #support::ComponentCodecError> {
                #component_defaults
                ::core::result::Result::Ok(#support::TypeSchema::new(
                    #support::ComponentTypeId::new(<Self as #support::Schema>::COMPONENT_ID),
                    #primary_alias,
                    #support::ComponentSchemaVersion::new(#version_value)
                        .ok_or_else(|| #support::ComponentCodecError::Message("zero schema version".into()))?,
                )
                #additional_aliases
                #group
                .with_postcard_codec()
                .with_capabilities([#(#component_capability_tokens),*])
                .with_fields([#(#field_schemas),*])
                .with_field_tombstones([
                    #(#support::ComponentFieldId::new(#tombstone_values)),*
                ])
                #authoring_initial)
            }

            fn decode_component(
                value: #support::ComponentValueRef<'_>,
            ) -> ::core::result::Result<Self, #support::ComponentCodecError> {
                let schema = <Self as #support::Schema>::type_schema()?;
                #support::validate_component_value(&schema, value)?;
                ::core::result::Result::Ok(#component_decoder)
            }

            fn encode_component(
                &self,
            ) -> ::core::result::Result<#support::ComponentValue, #support::ComponentCodecError> {
                let value = #component_encoder;
                #support::validate_component_value(&<Self as #support::Schema>::type_schema()?, &value)?;
                ::core::result::Result::Ok(value)
            }

            #field_access
            #postcard
        }
    })
}

fn field_accessors(fields: &[PersistentField], support: &TokenStream2) -> TokenStream2 {
    if fields.is_empty() {
        return TokenStream2::new();
    }
    let readers = fields.iter().map(|field| {
        let ident = &field.ident;
        let id = &field.id;
        quote!(#id => #support::encode_persistent_field(&self.#ident)?)
    });
    let writers = fields.iter().enumerate().map(|(index, field)| {
        let ident = &field.ident;
        let id = &field.id;
        quote!(#id => #support::update_schema_field(
            &mut self.#ident, &relative, value, &<Self as #support::Schema>::type_schema()?.fields()[#index],
        ))
    });
    quote! {
        fn get_field(&self, path: &#support::ComponentFieldPath)
            -> ::core::result::Result<#support::ComponentValue, #support::ComponentCodecError>
        {
            if path.is_empty() {
                return <Self as #support::Schema>::encode_component(self);
            }
            let (field, relative) = #support::split_field_path(path)?;
            let value = match field {
                #(#readers,)*
                _ => return ::core::result::Result::Err(#support::ComponentCodecError::missing_field(field)),
            };
            if relative.is_empty() {
                ::core::result::Result::Ok(value)
            } else {
                ::core::result::Result::Ok(value.read_path(&relative)?)
            }
        }

        fn set_field(&mut self, path: &#support::ComponentFieldPath, value: #support::ComponentValue)
            -> ::core::result::Result<(), #support::ComponentCodecError>
        {
            let (field, relative) = #support::split_field_path(path)?;
            match field {
                #(#writers,)*
                _ => ::core::result::Result::Err(#support::ComponentCodecError::missing_field(field)),
            }
        }
    }
}

fn postcard_methods(
    fields: &[PersistentField],
    skipped_decoders: &[TokenStream2],
    is_unit: bool,
    support: &TokenStream2,
) -> TokenStream2 {
    let writes = fields.iter().map(|field| {
        let ident = &field.ident;
        let id = &field.id;
        quote!(#support::write_postcard_field(&mut bytes, &self.#ident, #id)?;)
    });
    let reads = fields.iter().map(|field| {
        let ident = &field.ident;
        let ty = &field.ty;
        let id = &field.id;
        quote!(#ident: #support::read_postcard_field::<#ty>(&mut bytes, #id)?)
    });
    let references = fields.iter().map(|field| {
        let ident = &field.ident;
        quote!(#support::visit_postcard_field_references(&self.#ident, _visit)?;)
    });
    let constructed = if is_unit {
        quote!(Self)
    } else {
        quote!(Self { #(#reads,)* #(#skipped_decoders,)* })
    };
    let write_checks = postcard_checks(fields, quote!(self), support);
    let read_checks = postcard_checks(fields, quote!(component), support);
    let mutable = (!fields.is_empty()).then(|| quote!(mut));
    quote! {
        #support::__schema_postcard_methods! {
            fn encode_postcard(&self) -> ::core::result::Result<::std::vec::Vec<u8>, #support::ComponentCodecError> {
                #write_checks
                let #mutable bytes = ::std::vec::Vec::new();
                #(#writes)*
                ::core::result::Result::Ok(bytes)
            }

            fn decode_postcard(#mutable bytes: &[u8]) -> ::core::result::Result<Self, #support::ComponentCodecError> {
                let component = #constructed;
                #support::finish_postcard(bytes)?;
                #read_checks
                ::core::result::Result::Ok(component)
            }

            fn visit_postcard_references(
                &self,
                _visit: &mut dyn ::core::ops::FnMut(#support::PostcardReference),
            ) -> ::core::result::Result<(), #support::ComponentCodecError> {
                #(#references)*
                ::core::result::Result::Ok(())
            }
        }
    }
}

fn postcard_checks(
    fields: &[PersistentField],
    receiver: TokenStream2,
    support: &TokenStream2,
) -> TokenStream2 {
    let checks = fields.iter().map(|field| {
        let id = &field.id;
        let ident = &field.ident;
        let ty = &field.ty;
        let choices = field.metadata.choices.as_ref().map(|choices| quote! {
            #support::check_postcard_choice(&#receiver.#ident, #id, &[#(#choices),*])?;
        });
        let range = field.range.as_ref().map(|range| {
            let min = range.min.as_ref().map(|(inclusive, bound)| {
                let compare = if *inclusive { quote!(<) } else { quote!(<=) };
                quote! {
                    let minimum: #ty = #bound;
                    if #receiver.#ident #compare minimum {
                        return ::core::result::Result::Err(#support::ComponentCodecError::invalid_field(#id, "value in schema range"));
                    }
                }
            });
            let max = range.max.as_ref().map(|(inclusive, bound)| {
                let compare = if *inclusive { quote!(>) } else { quote!(>=) };
                quote! {
                    let maximum: #ty = #bound;
                    if #receiver.#ident #compare maximum {
                        return ::core::result::Result::Err(#support::ComponentCodecError::invalid_field(#id, "value in schema range"));
                    }
                }
            });
            quote!({ #min #max })
        });
        quote!(#choices #range)
    });
    quote!(#(#checks)*)
}

fn parse_component_args(attributes: &[Attribute]) -> Result<ComponentArgs> {
    let mut args = ComponentArgs::default();
    for attribute in attributes
        .iter()
        .filter(|attribute| attribute.path().is_ident("schema"))
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
            } else if meta.path.is_ident("group") {
                set_once(
                    &mut args.group,
                    meta.value()?.parse()?,
                    meta.path.span(),
                    "group",
                )
            } else if meta.path.is_ident("default") {
                let span = meta.path.span();
                set_once(&mut args.use_default, span, span, "default")
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
                Err(meta.error("unsupported Schema attribute"))
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
        .filter(|attribute| attribute.path().is_ident("schema"))
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
            } else if meta.path.is_ident("asset_demand") {
                let ident: Ident = meta.value()?.parse()?;
                let demand = AssetDemand::parse(&ident)?;
                set_once(
                    &mut args.asset_demand,
                    (demand, ident.span()),
                    meta.path.span(),
                    "asset_demand",
                )
            } else if args.metadata.parse(&meta)? {
                Ok(())
            } else {
                Err(meta.error("unsupported persistent field attribute"))
            }
        })?;
    }
    if let Some(span) = args.metadata.skip
        && (args.id.is_some()
            || !args.aliases.is_empty()
            || args.capabilities.is_some()
            || args.initial.is_some()
            || args.range.is_some()
            || args.asset_demand.is_some()
            || args.metadata.has_editor_metadata())
    {
        return Err(Error::new(
            span,
            "skip cannot be combined with authored field metadata",
        ));
    }
    let id = args
        .id
        .unwrap_or_else(|| LitStr::new(&ident.unraw().to_string(), ident.span()));
    if args.aliases.is_empty() {
        args.aliases.push(id.clone());
    }
    let mut capabilities = args
        .capabilities
        .unwrap_or_else(|| default_capabilities.to_vec());
    validate_field_subset(&capabilities, component_capabilities, ident.span())?;
    if args.metadata.read_only.is_some() {
        capabilities.retain(|capability| *capability != Capability::Edit);
    }
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
        asset_demand: args.asset_demand,
        metadata: args.metadata,
    })
}

fn parse_numeric_range(meta: syn::meta::ParseNestedMeta<'_>) -> Result<NumericRange> {
    if meta.input.peek(Token![=]) {
        let range: ExprRange = meta.value()?.parse()?;
        let inclusive = matches!(range.limits, syn::RangeLimits::Closed(_));
        let min = range
            .start
            .map(|value| numeric_literal(*value).map(|value| (true, value)))
            .transpose()?;
        let max = range
            .end
            .map(|value| numeric_literal(*value).map(|value| (inclusive, value)))
            .transpose()?;
        if min.is_none() && max.is_none() {
            return Err(meta.error("range requires at least one bound"));
        }
        return Ok(NumericRange { min, max });
    }
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
        let value = numeric_literal(bound.value()?.parse()?)?;
        *side = Some((inclusive, value));
        Ok(())
    })?;
    if range.min.is_none() && range.max.is_none() {
        return Err(meta.error("range requires at least one bound"));
    }
    Ok(range)
}

fn numeric_literal(value: Expr) -> Result<Expr> {
    if matches!(
        initial_scalar_literal(&value),
        Ok(Lit::Int(_) | Lit::Float(_))
    ) {
        Ok(value)
    } else {
        Err(Error::new_spanned(
            value,
            "range bounds must be numeric literals or negative numeric literals",
        ))
    }
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

fn finite_literal_checks(name: &Ident, fields: &[PersistentField]) -> TokenStream2 {
    let mut checks = TokenStream2::new();
    for field in fields {
        if let Some(initial) = &field.initial {
            checks.extend(finite_literal_check(
                name,
                &field.ident,
                initial,
                "authoring initial float must be finite for its field type",
            ));
        }
        if let Some(range) = &field.range {
            for (_, value) in [range.min.as_ref(), range.max.as_ref()]
                .into_iter()
                .flatten()
            {
                checks.extend(finite_literal_check(
                    name,
                    &field.ident,
                    value,
                    "range bounds must be finite for their field type",
                ));
            }
        }
    }
    checks
}

fn finite_literal_check(name: &Ident, field: &Ident, value: &Expr, message: &str) -> TokenStream2 {
    if !matches!(initial_scalar_literal(value), Ok(literal) if is_float_literal(literal)) {
        return TokenStream2::new();
    }
    // The unselected field access preserves its actual Rust type, including aliases and Self.
    // An item-level constant is evaluated by cargo check even when no schema is constructed.
    quote_spanned! {value.span()=>
        const _: () = {
            let value = match ::core::option::Option::<&#name>::None {
                ::core::option::Option::Some(component) => component.#field,
                ::core::option::Option::None => #value,
            };
            ::core::assert!(value.is_finite(), #message);
        };
    }
}

fn asset_demand_checks(
    name: &Ident,
    fields: &[PersistentField],
    support: &TokenStream2,
) -> TokenStream2 {
    let mut checks = TokenStream2::new();
    for field in fields {
        let Some((_, span)) = field.asset_demand else {
            continue;
        };
        let ty = &field.ty;
        // The local impl preserves Self in associated field types without inspecting type names.
        // Forcing its constant from an item-level constant checks even unused schema providers.
        checks.extend(quote_spanned! {span=>
            const _: () = {
                trait __NaraAssetDemandCheck {
                    const VALID: ();
                }
                impl __NaraAssetDemandCheck for #name {
                    const VALID: () = ::core::assert!(
                        <#ty as #support::PersistentFieldCodec>::HAS_ASSET_REFERENCES,
                        "asset_demand requires at least one AssetRef leaf in the field type"
                    );
                }
                <#name as __NaraAssetDemandCheck>::VALID
            };
        });
    }
    checks
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

fn field_schema(
    field: &PersistentField,
    support: &TokenStream2,
    use_default: bool,
) -> TokenStream2 {
    let ty = &field.ty;
    let shape = quote!(<#ty as #support::PersistentFieldCodec>::value_shape());
    let shape = match field.asset_demand {
        Some((demand, span)) => {
            let demand = demand.tokens(support);
            quote_spanned! {span=>
                #shape.with_asset_reference_demand(#demand)
                    .map_err(|error| #support::ComponentCodecError::Message(error.to_string()))?
            }
        }
        None => shape,
    };
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
                let variant = if *inclusive {
                    format_ident!("Included")
                } else {
                    format_ident!("Excluded")
                };
                quote!(::core::option::Option::Some(#support::ComponentNumericBound::#variant({
                    let bound: #ty = #value;
                    bound
                })))
            }
        };
        let min = bound_tokens(&range.min);
        let max = bound_tokens(&range.max);
        quote! { .with_numeric_interval(
            <#ty as #support::PersistentNumericField>::numeric_interval(#min, #max)?
        ) }
    });
    let ident = &field.ident;
    let default_value = match &field.metadata.default {
        Some(default) => {
            let value = metadata::rust_default(Some(default), ty);
            quote!(::core::option::Option::Some(#support::encode_persistent_field(&#value)?))
        }
        None if use_default => {
            quote!(::core::option::Option::Some(#support::encode_persistent_field(&schema_defaults.#ident)?))
        }
        None => quote!(<#ty as #support::PersistentFieldCodec>::default_value()),
    };
    let field_metadata = field.metadata.tokens(support);
    let editor_hint = if field.metadata.color_srgb.is_some() {
        quote!(#support::ComponentFieldEditorHint::LinearRgba)
    } else {
        quote!(<#ty as #support::PersistentFieldCodec>::EDITOR_HINT)
    };
    quote! {
        {
            let field = #support::ComponentFieldSchema::required(
                #support::ComponentFieldId::new(#id),
                #primary_alias,
                #support::ComponentFieldPath::from_fields([#id]),
                #shape,
            )
            #additional_aliases
            .with_capabilities([#(#capability_tokens),*])
            .with_reference_capabilities()
            .with_editor_hint(#editor_hint)
            #interval
            #field_metadata;
            match #default_value {
                ::core::option::Option::Some(value) => field.with_default_value(value),
                ::core::option::Option::None => field,
            }
        }
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
        "Schema requires a direct nara or nara_reflect dependency; use #[schema(crate = \"path\")] to override it",
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
