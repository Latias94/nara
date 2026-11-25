//! Derives stable material parameters together with their actual encase layout.

use std::collections::BTreeSet;

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Error, Fields, LitStr, Path, Result, parse_macro_input, parse_quote};

/// Implements material values, reconstruction and encase's complete `ShaderType`.
///
/// Each named field requires `#[material(id = "stable-field-id")]`. Supported
/// field types are `f32`, `i32`, `u32` and `glam::Vec2`, `Vec3`, `Vec4`; aliases
/// work through the sealed field codec. Arrays are not WGSL vectors.
///
/// A renamed direct `nara_render` or root `nara` dependency is detected. Use
/// `#[material(crate = "engine::material")]` for an explicit facade path, or
/// `#[material(crate = "materials")]` for a direct material crate. The path
/// must expose material `__macro_support`. Unquoted paths are accepted too.
#[proc_macro_derive(MaterialParameters, attributes(material))]
pub fn derive_material_parameters(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn expand(input: DeriveInput) -> Result<TokenStream2> {
    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        return Err(Error::new_spanned(
            &input.generics,
            "MaterialParameters does not support generic structs or where clauses",
        ));
    }
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) if !fields.named.is_empty() => &fields.named,
            _ => {
                return Err(Error::new_spanned(
                    &input.ident,
                    "MaterialParameters requires a nonempty struct with named fields",
                ));
            }
        },
        _ => {
            return Err(Error::new_spanned(
                &input.ident,
                "MaterialParameters requires a struct with named fields",
            ));
        }
    };
    let mut explicit = None;
    for attribute in &input.attrs {
        if !attribute.path().is_ident("material") {
            continue;
        }
        attribute.parse_nested_meta(|meta| {
            if !meta.path.is_ident("crate") {
                return Err(meta.error("expected #[material(crate = \"path\")] on the struct"));
            }
            if explicit.is_some() {
                return Err(meta.error("duplicate material crate override"));
            }
            let value = meta.value()?;
            explicit = Some(if value.peek(LitStr) {
                value.parse::<LitStr>()?.parse::<Path>()?
            } else {
                value.parse::<Path>()?
            });
            Ok(())
        })?;
    }
    let mut ids = BTreeSet::new();
    let mut declarations = Vec::new();
    let mut encoders = Vec::new();
    let mut decoders = Vec::new();
    let mut field_data = Vec::new();
    for field in fields {
        let ident = field
            .ident
            .as_ref()
            .ok_or_else(|| Error::new(field.span(), "MaterialParameters requires named fields"))?;
        let mut id = None;
        for attribute in &field.attrs {
            if attribute.path().is_ident("shader") {
                return Err(Error::new_spanned(
                    attribute,
                    "MaterialParameters does not support custom shader layout attributes",
                ));
            }
            if !attribute.path().is_ident("material") {
                continue;
            }
            attribute.parse_nested_meta(|meta| {
                if !meta.path.is_ident("id") {
                    return Err(meta.error("expected #[material(id = \"stable-field-id\")]"));
                }
                if id.is_some() {
                    return Err(meta.error("duplicate material field id attribute"));
                }
                id = Some(meta.value()?.parse::<LitStr>()?);
                Ok(())
            })?;
        }
        let id = id.ok_or_else(|| {
            Error::new_spanned(
                ident,
                "every material field requires #[material(id = \"stable-field-id\")]",
            )
        })?;
        if id.value().trim().is_empty() {
            return Err(Error::new_spanned(
                &id,
                "material field id must not be empty",
            ));
        }
        if !ids.insert(id.value()) {
            return Err(Error::new_spanned(
                &id,
                "duplicate material semantic field id",
            ));
        }
        field_data.push((ident, &field.ty, id));
    }
    let support = support_path(explicit)?;
    let name = &input.ident;
    for (index, (ident, ty, id)) in field_data.iter().enumerate() {
        let shader_name = LitStr::new(&ident.unraw().to_string(), ident.span());
        declarations.push(quote! {
            #support::MaterialUniformField::new(
                #id,
                #shader_name,
                <#ty as #support::MaterialParameterField>::KIND,
                #support::material_uniform_offset(
                    <Self as #support::encase::ShaderType>::METADATA.offset(#index)
                )?,
            )?
        });
        encoders.push(quote! {
            values.insert(
                ::std::string::String::from(#id),
                #support::material_parameter_value::<#ty>(#id, &self.#ident)?,
            );
        });
        decoders.push(quote! {
            #ident: #support::material_parameter_from_values::<#ty>(#id, values)?
        });
    }
    let encase_root = parse_quote!(#support::encase);
    let shader_type = uniform_implementation(&input, &encase_root)?;
    Ok(quote! {
        #shader_type

        impl #support::MaterialParameters for #name {
            fn parameter_layout() -> ::core::result::Result<
                #support::MaterialParameterLayout,
                #support::MaterialDeclarationError,
            > {
                let byte_size = #support::material_uniform_size(
                    <Self as #support::encase::ShaderType>::min_size().get()
                )?;
                #support::MaterialParameterLayout::new(
                    ::std::vec![#(#declarations),*],
                    byte_size,
                )
            }

            fn parameter_values(&self) -> ::core::result::Result<
                #support::MaterialParameterValues,
                #support::MaterialDeclarationError,
            > {
                let layout = <Self as #support::MaterialParameters>::parameter_layout()?;
                let mut values = #support::MaterialParameterValues::new();
                #(#encoders)*
                layout.validate_values(&values)?;
                ::core::result::Result::Ok(values)
            }

            fn from_parameter_values(
                values: &#support::MaterialParameterValues,
            ) -> ::core::result::Result<Self, #support::MaterialDeclarationError> {
                let layout = <Self as #support::MaterialParameters>::parameter_layout()?;
                layout.validate_values(values)?;
                ::core::result::Result::Ok(Self { #(#decoders),* })
            }
        }
    })
}

/// Delegates uniform layout and writing without opting into Encase's unrelated buffer readers.
/// Transparent type groups leave aliases to Rust rather than Encase's syntactic ArrayLength
/// special case. Semantic reconstruction remains owned by `from_parameter_values`.
fn uniform_implementation(input: &DeriveInput, root: &Path) -> Result<TokenStream2> {
    let mut input = input.clone();
    if let Data::Struct(data) = &mut input.data {
        for field in &mut data.fields {
            field.ty = syn::Type::Group(syn::TypeGroup {
                group_token: Default::default(),
                elem: Box::new(field.ty.clone()),
            });
        }
    }
    let generated = encase_derive_impl::derive_shader_type(input, root);
    let mut generated: syn::File = syn::parse2(generated)?;
    generated.items.retain(|item| {
        !matches!(item, syn::Item::Impl(implementation)
            if implementation.trait_.as_ref().and_then(|(_, path, _)| path.segments.last())
                .is_some_and(|segment| segment.ident == "CreateFrom" || segment.ident == "ReadFrom"))
    });
    Ok(quote!(#generated))
}

fn support_path(explicit: Option<Path>) -> Result<Path> {
    if let Some(path) = explicit {
        return Ok(parse_quote!(#path::__macro_support));
    }
    for package in ["nara_render", "nara"] {
        let root: Path = match crate_name(package) {
            Ok(FoundCrate::Itself)
                if std::env::var("CARGO_CRATE_NAME").as_deref() == Ok(package) =>
            {
                parse_quote!(crate)
            }
            Ok(FoundCrate::Itself) => {
                let dependency = Ident::new(package, Span::call_site());
                parse_quote!(::#dependency)
            }
            Ok(FoundCrate::Name(name)) => {
                let dependency = Ident::new(&name, Span::call_site());
                parse_quote!(::#dependency)
            }
            Err(_) => continue,
        };
        return Ok(parse_quote!(#root::material::__macro_support));
    }
    Err(Error::new(
        Span::call_site(),
        "MaterialParameters requires a direct nara_render or nara dependency; use #[material(crate = \"path\")] to override it",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(input: DeriveInput) -> String {
        match expand(input) {
            Ok(_) => panic!("invalid material declaration was accepted"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn rejects_missing_or_duplicate_semantic_ids() {
        assert!(
            error(parse_quote! {
                struct Missing {
                    value: f32,
                }
            })
            .contains("every material field requires")
        );
        assert!(
            error(parse_quote! {
                struct Duplicate {
                    #[material(id = "value")]
                    first: f32,
                    #[material(id = "value")]
                    second: f32,
                }
            })
            .contains("duplicate material semantic field id")
        );
        assert!(
            error(parse_quote! {
                struct Blank {
                    #[material(id = " \t")]
                    value: f32,
                }
            })
            .contains("must not be empty")
        );
    }

    #[test]
    fn rejects_shapes_before_invoking_encase() {
        for input in [
            parse_quote!(
                struct Empty {}
            ),
            parse_quote!(
                struct Unit;
            ),
            parse_quote!(
                struct Tuple(f32);
            ),
            parse_quote!(
                enum Choice {
                    One,
                }
            ),
            parse_quote!(union Bits { value: f32 }),
        ] {
            assert!(error(input).contains("named fields"));
        }
        assert!(
            error(parse_quote! {
                struct Generic<T> {
                    #[material(id = "value")]
                    value: T,
                }
            })
            .contains("does not support generic structs")
        );
        assert!(
            error(parse_quote! {
                struct Borrowed<'a> {
                    #[material(id = "value")]
                    value: &'a f32,
                }
            })
            .contains("does not support generic structs")
        );
    }

    #[test]
    fn rejects_misplaced_unknown_or_repeated_attributes() {
        for input in [
            parse_quote! {
                #[material(id = "container")]
                struct Container {
                    #[material(id = "value")]
                    value: f32,
                }
            },
            parse_quote! {
                #[material(crate = "first", crate = "second")]
                struct Container {
                    #[material(id = "value")]
                    value: f32,
                }
            },
            parse_quote! {
                struct Field {
                    #[material(id = "value", id = "other")]
                    value: f32,
                }
            },
            parse_quote! {
                struct Field {
                    #[material(id = "value", name = "renamed")]
                    value: f32,
                }
            },
            parse_quote! {
                struct Field {
                    #[material(id = "value")]
                    #[shader(size(runtime))]
                    value: f32,
                }
            },
        ] {
            assert!(!error(input).is_empty());
        }
    }

    #[test]
    fn explicit_facades_and_raw_identifiers_expand_as_rust_items() {
        for input in [
            parse_quote! {
                #[material(crate = "::renamed_engine::material")]
                struct Palette {
                    #[material(id = "palette.tint")]
                    tint: glam::Vec4,
                }
            },
            parse_quote! {
                #[material(crate = ::renamed_material)]
                struct RawIdentifier {
                    #[material(id = "stable.identity")]
                    r#type: f32,
                }
            },
        ] {
            let expanded = expand(input).expect("valid declaration expands");
            syn::parse2::<syn::File>(expanded).expect("derive emits complete Rust items");
        }
    }
}
