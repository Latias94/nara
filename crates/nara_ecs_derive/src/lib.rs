use bevy_ecs_macro_logic::component::{DeriveComponent, StorageAttribute, StorageTy};
use bevy_macro_utils::derive_label;
use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
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
    let ecs = ecs_path()?;
    component.impl_component(&mut input, &ecs, StorageTy::Table)
}

#[proc_macro_derive(Resource, attributes(component, require))]
pub fn derive_resource(input: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(input as DeriveInput);
    expand_resource(&mut input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn expand_resource(input: &mut DeriveInput) -> Result<TokenStream2> {
    let ecs = ecs_path()?;
    let mut component = DeriveComponent::parse(input, StorageAttribute::Disallowed)?;
    let name = &input.ident;
    let (_, type_generics, _) = input.generics.split_for_impl();
    component.additional_requires.push(quote! {
        let resource_component_id = if let ::core::option::Option::Some(id) =
            required_components.components_registrator().component_id::<#name #type_generics>()
        {
            id
        } else {
            required_components.components_registrator().register_component::<#name #type_generics>()
        };
        required_components.register_required::<#ecs::resource::IsResource>(move || {
            #ecs::resource::IsResource::new(resource_component_id)
        });
    });
    let component_impl = component.impl_component(input, &ecs, StorageTy::SparseSet)?;
    let name = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        #component_impl
        impl #impl_generics #ecs::resource::Resource for #name #type_generics #where_clause {}
    })
}

#[proc_macro_derive(ScheduleLabel)]
pub fn derive_schedule_label(input: TokenStream) -> TokenStream {
    derive_ecs_label(input, "ScheduleLabel")
}

#[proc_macro_derive(SystemSet)]
pub fn derive_system_set(input: TokenStream) -> TokenStream {
    derive_ecs_label(input, "SystemSet")
}

fn derive_ecs_label(input: TokenStream, trait_name: &str) -> TokenStream {
    let parsed = parse_macro_input!(input as DeriveInput);
    let mut path = match ecs_path() {
        Ok(path) => path,
        Err(error) => return error.into_compile_error().into(),
    };
    path.segments.push(parse_quote!(schedule));
    path.segments
        .push(Ident::new(trait_name, Span::call_site()).into());
    derive_label(parsed, trait_name, &path)
}

fn ecs_path() -> Result<Path> {
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
        "nara ECS derives require a direct nara or nara_ecs dependency",
    ))
}
