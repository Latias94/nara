//! Mechanical callbacks over an explicit current component inventory.

use crate::{ComponentRegistryError, ComponentSchema, ComponentSchemaCatalog};

#[doc(hidden)]
pub fn validate_inventory(
    schemas: Vec<ComponentSchema>,
) -> Result<Vec<ComponentSchema>, ComponentRegistryError> {
    let mut catalog = ComponentSchemaCatalog {
        components: schemas,
        ..Default::default()
    };
    crate::registry::validate_catalog_contents(&catalog)?;
    catalog.canonicalize();
    Ok(catalog.components)
}

/// Generates schema and native registration callbacks from one current type list.
///
/// ```ignore
/// nara_reflect::persistent_components! {
///     pub mod agent_types = [Health, Cooldown];
/// }
/// ```
///
/// Entries are type identifiers imported or aliased in the invocation module. Import qualified
/// paths before listing them; generated callbacks preserve that explicit invocation scope.
///
/// `schemas()` returns validated current schemas. `initial_catalog()` is only a generation-one
/// provider source. Successors build their explicit catalog from `schemas()` and keep historical
/// membership and migrations separate. `register()` adds current bindings to its supplied
/// candidate; the existing provider owner is responsible for atomic installation. No World,
/// discovery, freeze or product installation is performed by these callbacks.
#[macro_export]
macro_rules! persistent_components {
    ($vis:vis mod $name:ident = [$($component:ident),+ $(,)?];) => {
        $vis mod $name {
            pub fn schemas() -> ::core::result::Result<
                ::std::vec::Vec<$crate::ComponentSchema>, $crate::ComponentRegistryError,
            > {
                $crate::__macro_support::validate_inventory(::std::vec![
                    $(<super::$component as $crate::PersistentComponentProvider>::persistent_component_schema()),+
                ])
            }

            pub fn initial_catalog() -> ::core::result::Result<
                $crate::ComponentSchemaCatalog, $crate::ComponentSchemaProviderSourceError,
            > {
                ::core::result::Result::Ok($crate::ComponentSchemaCatalog {
                    components: schemas().map_err(|_| $crate::ComponentSchemaProviderSourceError::new(
                        "component.inventory.invalid",
                    ))?,
                    ..::core::default::Default::default()
                })
            }

            pub fn register(registry: &mut $crate::ComponentRegistry)
                -> ::core::result::Result<(), $crate::ComponentRegistryError>
            {
                $(registry.register_persistent_component::<super::$component>()?;)+
                ::core::result::Result::Ok(())
            }
        }
    };
}
