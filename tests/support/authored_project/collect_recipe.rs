pub fn product_recipe() -> Result<nara::ProductRecipe, nara::ProductRecipeError> {
    let recipe = nara::ProductRecipe::new()
        .add_runtime_image_codec::<nara::image::ImageRuntimeCodec>(
            nara::runtime_content::RuntimeCodecDefinitionId::new(
                "nara.starter.desktop-2d.image",
                1,
            ),
        )?
        .add_plugin::<first_step::FirstStepPlugin>()?
        .add_plugin::<nara::scene::fixed_spatial::FixedSpatialPlugin>()?
        .add_contribution(collect_gameplay::contribution()?)?;
    #[cfg(feature = "desktop")]
    let recipe = recipe.add_plugin::<collect_gameplay::DesktopInputPlugin>()?;
    Ok(recipe)
}
