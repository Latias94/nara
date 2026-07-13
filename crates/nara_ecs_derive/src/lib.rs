use bevy_ecs_macro_logic::component::{DeriveComponent, StorageAttribute, StorageTy};
use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use syn::{DeriveInput, Error, Ident, Path, Result, parse_macro_input, parse_quote};

#[proc_macro_derive(
    Component,
    attributes(component, require, relationship, relationship_target, entities)
)]
pub fn derive_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_component(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn expand_component(mut input: DeriveInput) -> Result<TokenStream2> {
    let component = DeriveComponent::parse(&input, StorageAttribute::Allowed)?;
    let ecs = component_ecs_path()?;
    component.impl_component(&mut input, &ecs, StorageTy::Table)
}

fn component_ecs_path() -> Result<Path> {
    for package in ["nara_ecs", "nara"] {
        match crate_name(package) {
            Ok(FoundCrate::Itself) => {
                return Ok(if package == "nara_ecs" {
                    parse_quote!(crate)
                } else {
                    parse_quote!(crate::ecs)
                });
            }
            Ok(FoundCrate::Name(name)) => {
                let dependency = Ident::new(&name, Span::call_site());
                return if package == "nara_ecs" {
                    Ok(parse_quote!(::#dependency))
                } else {
                    Ok(parse_quote!(::#dependency::ecs))
                };
            }
            Err(_) => {}
        }
    }

    Err(Error::new(
        Span::call_site(),
        "Component requires a direct nara or nara_ecs dependency",
    ))
}
