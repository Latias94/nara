# Bevy ECS Derive Extraction

`src/upstream/bundle.rs` and `src/upstream/system_param.rs` are bounded extractions
of `crates/bevy_ecs/macros/src/lib.rs` from the **published**
`bevy_ecs_macros` **0.20.0** crate. Its `.cargo_vcs_info.json` identifies commit
`b3bdd75becaffe9a16677cf5bae5bd6d198f052b`.

Published crate archive SHA-256:
`3dde5305169503c9101404bf8cf17e8351f56fccf5152b8e5d7d7c32dcf1c486`.

Upstream source:
<https://github.com/bevyengine/bevy/blob/b3bdd75becaffe9a16677cf5bae5bd6d198f052b/crates/bevy_ecs/macros/src/lib.rs>

The extraction retains the upstream MIT OR Apache-2.0 license. Both unmodified
license texts accompany it in this directory. No other upstream macro modules
are copied.

The changes are deliberately limited:

- Accept a `syn::Path` to the facade's hidden complete substrate support instead
  of discovering Bevy through `BevyManifest`.
- Move proc-macro entry points to Nara's existing derive crate and return Bundle
  parse errors through `syn::Result`.
- Include lifetime and const generic parameters in Bundle's precise RPIT capture
  list, alongside type parameters. The published source includes only types,
  which fails for an otherwise valid const-generic Bundle.

Generated storage, pointer movement, field ordering, validation, access
registration, deferred application, read-only delegation and SystemParamBuilder
behavior remain upstream-owned semantics. Do not rewrite generated token strings
or introduce a second ECS implementation.

The derive entry points resolve the direct `nara_ecs` or `nara` dependency by
package identity, including renamed dependencies, and append
`::__private::bevy_ecs`. That hidden re-export supplies every upstream module
needed by generated code without making those modules part of the ordinary
authoring surface. Advanced source consumers use the public `upstream` path;
generated code uses the hidden support path.

Keep these sources synchronized with `bevy_ecs`, `bevy_ecs_macro_logic` and
`bevy_macro_utils`. Any dependency upgrade must review the exact new released
source, compare both extractions and execute the renamed-root and renamed-ECS
derive fixtures, including const generics and deferred commands. A development
checkout of Bevy is not an interchangeable patch baseline.

At this pinned release, `bevy_ecs_macro_logic` provides path-parameterized
Component/Resource expansion and `bevy_macro_utils` provides label expansion.
Bundle/SystemParam do not expose equivalent helpers, which is why only those
two derive implementations are extracted here. These are implementation choices
for the pinned release, not requirements on a future upstream macro API.
