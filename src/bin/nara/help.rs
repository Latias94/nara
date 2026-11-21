use super::args::HelpTopic;

pub(crate) const fn text(topic: HelpTopic) -> &'static str {
    match topic {
        HelpTopic::Check => CHECK,
        HelpTopic::Schema => SCHEMA,
        HelpTopic::New => NEW,
        HelpTopic::Export => EXPORT,
        HelpTopic::Repair => REPAIR,
    }
}

const CHECK: &str = "\
Check a project's saved content with its compiled recipe without starting gameplay.

Usage:
  nara check <project-directory> [--profile <name>]
  nara check --help

Options:
  --profile <name>  Select a project launch profile.
  -h, --help        Show this help without opening or building the project.

Example (after creating my-game):
  nara check ./my-game
";

const SCHEMA: &str = "\
Check or write the authoring schema bundle from a project's compiled recipe.

Usage:
  nara schema <project-directory> (--check | --write) [--profile <name>]
  nara schema --help

Options:
  --check          Verify that the declared authoring schema bundle is current.
  --write          Write the declared authoring schema bundle.
  --profile <name> Select a project launch profile.
  -h, --help       Show this help without opening or building the project.

Example (after creating my-game):
  nara schema ./my-game --write
";

const NEW: &str = "\
Create a desktop 2D Rust project. Use desktop-2d/v2 for editable keyboard and gamepad bindings.

Usage:
  nara new desktop-2d/v2 <slug> --parent <directory> --local-checkout <directory>
  nara new desktop-2d/v2 <slug> --parent <directory> --git <https-url> --rev <object-id>
  nara new desktop-2d/v1 <slug> --parent <directory> --local-checkout <directory>
  nara new desktop-2d/v1 <slug> --parent <directory> --git <https-url> --rev <object-id>
  nara new --help

Options:
  --parent <directory>         Existing directory in which to create <slug>.
  --local-checkout <directory> Existing Nara checkout on the same filesystem volume.
  --git <https-url>            Nara repository; requires --rev instead of --local-checkout.
  --rev <object-id>            Immutable Git commit object ID; requires --git.
  -h, --help                  Show this help without creating a project.

Example (from a Nara checkout; creates my-game in its parent directory):
  nara new desktop-2d/v2 my-game --parent .. --local-checkout .

The desktop-2d/v1 camera-only starter is also available:
  nara new desktop-2d/v1 my-game --parent .. --local-checkout .
";

const EXPORT: &str = "\
Check, build, and publish a source-carrying project export.

Usage:
  nara export <project-directory> --parent <directory> --destination <name>
  nara export --help

Options:
  --parent <directory> Existing directory in which to publish the export.
  --destination <name> New export directory name within --parent.
  -h, --help           Show this help without opening, building, or exporting the project.

Example (after creating my-game; my-game-export must not exist):
  nara export ./my-game --parent . --destination my-game-export
";

const REPAIR: &str = "\
Repair a project's startup Scene selection using its compiled recipe.

Usage:
  nara repair startup-scene <project-directory> [--profile <name>] --replacement <scene-path>
  nara repair --help
  nara repair startup-scene --help

Options:
  --profile <name>           Select a project launch profile.
  --replacement <scene-path> Replacement Scene path relative to the project's Scene root.
  -h, --help                 Show this help without opening, building, or repairing the project.

Example (when the startup selection is missing and startup.scene.json exists):
  nara repair startup-scene ./my-game --replacement startup.scene.json
";
