//! Sources for v0 authored-content, Cook, and Player integration tests.

use std::{
    fs,
    path::{Path, PathBuf},
};

use nara::{
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetSourceKind,
        ImportedProductId, StableAssetId,
    },
    input::{
        ActionBinding, ActionContext, ActionId, ActionMapBuilder, ActionMapFileCandidate,
        ActionMapLimits, GamepadButton, KeyCode,
    },
    reflect::AuthoringSchemaBundle,
    render::RENDER_SCHEMA_PROVIDER,
    scene::{SCENE_COMPONENTS_SCHEMA_PROVIDER, TRANSFORM_SCHEMA_PROVIDER},
};

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum AuthoredFixture {
    Basic,
    Actions,
}

pub(crate) fn create_authored_project(
    checkout: &Path,
    parent: &Path,
    slug: &str,
    fixture: AuthoredFixture,
) -> PathBuf {
    let root = parent.join(slug);
    fs::create_dir(&root).unwrap();
    for directory in [
        "assets", "input", "prefabs", "scenes", "schema", "src", "src/bin",
    ] {
        fs::create_dir(root.join(directory)).unwrap();
    }
    let write = |path: &str, contents: &str| fs::write(root.join(path), contents).unwrap();
    let universe = StableAssetId::try_random().unwrap();
    let actions = fixture == AuthoredFixture::Actions;
    let checkout = checkout.canonicalize().unwrap();
    let nara_path = toml::Value::String(checkout.to_str().unwrap().to_owned());
    let winit_path =
        toml::Value::String(checkout.join("vendor/winit").to_str().unwrap().to_owned());
    let name = toml::Value::String(slug.to_owned());
    write(
        "Cargo.toml",
        &format!(
            include_str!("authored_project/Cargo.toml.in"),
            universe = universe,
            nara_path = nara_path,
            winit_path = winit_path,
            gamepad_feature = if actions { ", \"nara/gamepad\"" } else { "" },
        ),
    );
    write(
        "nara.toml",
        &format!(
            include_str!("authored_project/nara.toml.in"),
            name = name,
            universe = universe,
            input = if actions {
                "\n[input]\naction_map = \"input/starter.actions.json\"\n"
            } else {
                ""
            },
        ),
    );
    write("build.rs", include_str!("authored_project/build.rs"));
    write(
        "src/bin/headless.rs",
        include_str!("authored_project/headless.rs"),
    );
    write("src/bin/cook.rs", include_str!("authored_project/cook.rs"));
    write(
        "src/bin/player.rs",
        include_str!("authored_project/player.rs"),
    );
    match fixture {
        AuthoredFixture::Basic => {
            write("src/lib.rs", include_str!("authored_project/basic.rs"));
            write(
                "src/bin/desktop.rs",
                include_str!("authored_project/desktop.rs"),
            );
        }
        AuthoredFixture::Actions => {
            write("src/lib.rs", include_str!("authored_project/actions.rs"));
            write(
                "src/gameplay.rs",
                include_str!("authored_project/gameplay.rs"),
            );
            write(
                "src/bin/desktop.rs",
                include_str!("authored_project/actions_desktop.rs"),
            );
            write("input/starter.actions.json", &fixture_actions());
        }
    }
    let scene_name = "startup.scene.json";
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::try_random().unwrap(),
            AssetPath::new(scene_name).unwrap(),
            AssetSourceKind::Scene,
        ),
        (),
    );
    write(
        "scenes/startup.scene.json",
        include_str!("authored_project/startup.scene.json"),
    );
    write(
        "scenes/startup.scene.json.meta",
        &metadata.to_json_string().unwrap(),
    );
    let schema = AuthoringSchemaBundle::from_trusted_provider_definitions([
        SCENE_COMPONENTS_SCHEMA_PROVIDER,
        TRANSFORM_SCHEMA_PROVIDER,
        RENDER_SCHEMA_PROVIDER,
        nara::sprite::SPRITE_SCHEMA_PROVIDER,
        nara::tilemap::TILEMAP_SCHEMA_PROVIDER,
        nara::material::MATERIAL_SCHEMA_PROVIDER,
    ])
    .unwrap();
    write(
        "schema/authoring.bundle.json",
        &schema.to_json_string().unwrap(),
    );
    root.canonicalize().unwrap()
}

fn fixture_actions() -> String {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    let context = ActionContext::gameplay();
    builder.declare_context(context.clone(), true).unwrap();
    for (action, key, button) in [
        (
            "move.left",
            KeyCode::Character('a'),
            GamepadButton::DPadLeft,
        ),
        (
            "move.right",
            KeyCode::Character('d'),
            GamepadButton::DPadRight,
        ),
        ("move.up", KeyCode::Character('w'), GamepadButton::DPadUp),
        (
            "move.down",
            KeyCode::Character('s'),
            GamepadButton::DPadDown,
        ),
        ("confirm", KeyCode::Enter, GamepadButton::South),
    ] {
        let action = ActionId::new(action).unwrap();
        for binding in [
            ActionBinding::key(action.clone(), key),
            ActionBinding::gamepad_button(action, button.code()),
        ] {
            builder.bind(binding.with_context(context.clone())).unwrap();
        }
    }
    let product = AssetProductRef::new(
        StableAssetId::try_random().unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap();
    ActionMapFileCandidate::new(product, builder.compile().unwrap())
        .unwrap()
        .to_json_string()
        .unwrap()
}
