// Adapted from bevy_ecs_macros 0.20.0, crates/bevy_ecs/macros/src/lib.rs.
// Upstream commit: b3bdd75becaffe9a16677cf5bae5bd6d198f052b.
// Licensed under MIT OR Apache-2.0; see ../../upstream/bevy-0.20.0/.
// Keep runtime, macro helpers and this extraction synchronized; see that directory's README.

use bevy_macro_utils::{
    fq_std::{FQDefault, FQIterator, FQOption},
    get_struct_fields,
};
use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, format_ident, quote};
use syn::{DeriveInput, GenericParam};

enum BundleFieldKind {
    Component,
    Ignore,
}

const BUNDLE_ATTRIBUTE_NAME: &str = "bundle";
const BUNDLE_ATTRIBUTE_IGNORE_NAME: &str = "ignore";
const BUNDLE_ATTRIBUTE_NO_FROM_COMPONENTS: &str = "ignore_from_components";

#[derive(Debug)]
struct BundleAttributes {
    impl_from_components: bool,
}

impl Default for BundleAttributes {
    fn default() -> Self {
        Self {
            impl_from_components: true,
        }
    }
}

pub(crate) fn derive_bundle(ast: DeriveInput, ecs_path: &syn::Path) -> syn::Result<TokenStream2> {
    let mut attributes = BundleAttributes::default();

    for attr in &ast.attrs {
        if attr.path().is_ident(BUNDLE_ATTRIBUTE_NAME) {
            let parsing = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident(BUNDLE_ATTRIBUTE_NO_FROM_COMPONENTS) {
                    attributes.impl_from_components = false;
                    return Ok(());
                }

                Err(meta.error(format!("Invalid bundle container attribute. Allowed attributes: `{BUNDLE_ATTRIBUTE_NO_FROM_COMPONENTS}`")))
            });

            parsing?;
        }
    }

    let fields = get_struct_fields(&ast.data, "derive(Bundle)")?;

    let mut field_kinds = Vec::with_capacity(fields.len());

    for field in fields {
        let mut kind = BundleFieldKind::Component;

        for attr in field
            .attrs
            .iter()
            .filter(|a| a.path().is_ident(BUNDLE_ATTRIBUTE_NAME))
        {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident(BUNDLE_ATTRIBUTE_IGNORE_NAME) {
                    kind = BundleFieldKind::Ignore;
                    Ok(())
                } else {
                    Err(meta.error(format!(
                        "Invalid bundle attribute. Use `{BUNDLE_ATTRIBUTE_IGNORE_NAME}`"
                    )))
                }
            })?;
        }

        field_kinds.push(kind);
    }

    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();

    let mut active_field_types = Vec::new();
    let mut active_field_members = Vec::new();
    let mut active_field_locals = Vec::new();
    let mut inactive_field_members = Vec::new();
    for ((field_member, field_type), field_kind) in
        fields.members().zip(field_types).zip(field_kinds)
    {
        let field_local = format_ident!("field_{}", field_member);

        match field_kind {
            BundleFieldKind::Component => {
                active_field_types.push(field_type);
                active_field_locals.push(field_local);
                active_field_members.push(field_member);
            }
            BundleFieldKind::Ignore => inactive_field_members.push(field_member),
        }
    }
    let generics = ast.generics;
    // RPIT precise capture must include const and lifetime parameters as well as types.
    let generics_capture = generics.params.iter().map(|param| match param {
        GenericParam::Lifetime(param) => param.lifetime.to_token_stream(),
        GenericParam::Type(param) => param.ident.to_token_stream(),
        GenericParam::Const(param) => param.ident.to_token_stream(),
    });
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let struct_name = &ast.ident;

    let bundle_impl = quote! {
        // SAFETY:
        // - ComponentId is returned in field-definition-order. [get_components] uses field-definition-order
        // - `Bundle::get_components` is exactly once for each member. Rely's on the Component -> Bundle implementation to properly pass
        //   the correct `StorageType` into the callback.
        unsafe impl #impl_generics #ecs_path::bundle::Bundle for #struct_name #ty_generics #where_clause {
            fn component_ids(
                components: &mut #ecs_path::component::ComponentsRegistrator,
            ) -> impl #FQIterator<Item = #ecs_path::component::ComponentId> + use<#(#generics_capture,)*> {
                ::core::iter::empty()#(.chain(<#active_field_types as #ecs_path::bundle::Bundle>::component_ids(components)))*
            }

            fn get_component_ids(
                components: &#ecs_path::component::Components,
            ) -> impl #FQIterator<Item = #FQOption<#ecs_path::component::ComponentId>> {
                ::core::iter::empty()#(.chain(<#active_field_types as #ecs_path::bundle::Bundle>::get_component_ids(components)))*
            }
        }
    };

    let dynamic_bundle_impl = quote! {
        impl #impl_generics #ecs_path::bundle::DynamicBundle for #struct_name #ty_generics #where_clause {
            type Effect = ();
            #[allow(unused_variables)]
            #[allow(non_snake_case, reason = "deconstruct_moving_ptr uses #active_field_locals as a local binding name")]
            #[inline]
            unsafe fn get_components(
                ptr: #ecs_path::ptr::MovingPtr<'_, Self>,
                func: &mut impl ::core::ops::FnMut(#ecs_path::component::StorageType, #ecs_path::ptr::OwningPtr<'_>)
            ) {
                use #ecs_path::__macro_exports::DebugCheckedUnwrap;

                #ecs_path::ptr::deconstruct_moving_ptr!({
                    let #struct_name { #(#active_field_members: #active_field_locals,)* #(#inactive_field_members: _,)* } = ptr;
                });
                #(
                    <#active_field_types as #ecs_path::bundle::DynamicBundle>::get_components(
                        #active_field_locals,
                        func
                    );
                )*
            }

            #[allow(unused_variables)]
            #[inline]
            unsafe fn apply_effect(
                ptr: #ecs_path::ptr::MovingPtr<'_, ::core::mem::MaybeUninit<Self>>,
                func: &mut #ecs_path::world::EntityWorldMut<'_>,
            ) {
            }
        }
    };

    let fqdefault = FQDefault.into_token_stream();
    let from_components_impl = attributes.impl_from_components.then(|| quote! {
        // SAFETY:
        // - ComponentId is returned in field-definition-order. [from_components] uses field-definition-order
        unsafe impl #impl_generics #ecs_path::bundle::BundleFromComponents for #struct_name #ty_generics #where_clause {
            #[allow(unused_variables, non_snake_case)]
            unsafe fn from_components<__T, __F>(ctx: &mut __T, func: &mut __F) -> Self
            where
                __F: ::core::ops::FnMut(&mut __T) -> #ecs_path::ptr::OwningPtr<'_>
            {
                Self {
                    #(#active_field_members: <#active_field_types as #ecs_path::bundle::BundleFromComponents>::from_components(ctx, &mut *func),)*
                    #(#inactive_field_members: #fqdefault::default(),)*
                }
            }
        }
    });
    Ok(quote! {
        #bundle_impl
        #from_components_impl
        #dynamic_bundle_impl
    })
}
