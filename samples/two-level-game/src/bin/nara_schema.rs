use std::{
    error::Error,
    io::{self, Write},
};

use nara::{
    fs::RelativePath,
    project_host::{
        ingest_project_manifest, open_explicit_product_project_root, resolve_product_recipe,
    },
};
use two_level_game::product_recipe;

fn main() -> Result<(), Box<dyn Error>> {
    let root = open_explicit_product_project_root(env!("CARGO_MANIFEST_DIR"))?;
    let manifest = root.open_file(&RelativePath::new("nara.toml")?)?;
    let settings = ingest_project_manifest(&manifest, None)?;
    let plan = resolve_product_recipe(&settings, product_recipe())?;
    let catalog = plan.schema_validation().snapshot().catalog();
    writeln!(io::stdout().lock(), "{}", catalog.to_json_string()?)?;
    Ok(())
}
