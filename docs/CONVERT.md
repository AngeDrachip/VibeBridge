# Convert Java mods to Horizon content mods

`vibe horizon convert` turns one or several Java mods into Horizon `.hmod`
content bundles. A source may be a Forge/NeoForge jar or an unpacked mod folder.
Sources are opened read-only and never modified.

```powershell
# One bundle per mod, in ./converted/
vibe horizon convert mods/*.jar --output-dir converted

# Several mods, still one bundle each
vibe horizon convert first.jar second.jar --output-dir converted

# One merged bundle for the whole set
vibe horizon convert first.jar second.jar --output-dir converted `
  --merge mypack --name "My pack" --version 2.0

# Install the result
vibe horizon install converted/mypack.hmod
```

A source that fails (missing file, unreadable jar, no `mods.toml`, no claimed
namespace) is reported with its error and the other sources are still
converted. The command then exits `1`. In a `--merge`, one failed source stops
the whole merge: no bundle is written, so a mod is never silently dropped.

## What is carried, and what is not

Horizon is a Rust engine with no JVM. A `.hmod` is a data and asset bundle. The
conversion is therefore a **content export**, not a port of the mod's code.

| Carried | Left behind |
| --- | --- |
| `assets/<namespace>/` — models, textures, blockstates, item definitions, lang, sounds, atlases | Java classes and entrypoints (`@Mod`, loaders, proxies) |
| `data/<namespace>/` — recipes, loot tables, tags, advancements, structures, damage types | Mixins, coremods, access transformers |
| Additions to vanilla tags (unless `--no-vanilla-tags`) | Blocks, items, entities, menus, keybinds and commands registered in Java |
| Convention-tag additions and atlas sources (unless disabled) | World generation, rendering, GUIs, networking, gameplay logic |
| `pack.mcmeta` format declarations found at the jar root | Anything the mod computes at runtime |

Consequences you should expect:

- A block or item the mod registers **only in Java** does not exist in Horizon
  unless the jar also ships a `data/<id>/horizon/item|block` declaration, or you
  state one with `--item-template NAME=TEMPLATE`. There is no automatic
  detection: Java registration code is not executed during conversion.
- Recipes and tags that refer to such missing items are carried verbatim; they
  are inert until the item exists.
- Textures and models without a corresponding item or block stay unused.

`vibe horizon export <jar>` remains the single-jar form; `convert` adds batches,
folders, merging and a per-source report.

## Options

| Option | Effect |
| --- | --- |
| `--output-dir DIR` | Where bundles are written (default: current folder). |
| `--merge ID` | Put every source in one bundle with this id. |
| `--id ID` | Bundle id for a single source; refused with several sources. |
| `--name`, `--version` | Manifest metadata (merged bundle: defaults to `Converted pack (N mods)` / `1.0.0`). |
| `--side client\|server\|both` | Declared side. Defaults to each source's side; a merge that mixes sides becomes `both`. |
| `--namespace NS` | Claim an extra namespace (repeatable), for jars whose files live beside their mod id. |
| `--force` | Replace an existing `<id>.hmod`. |
| `--keep-first` | In a merge, keep the first source's copy when two sources ship the same path with different bytes. |
| `--no-vanilla-tags`, `--no-convention-tags`, `--no-atlases` | Leave those additions out. |
| `--item-template NAME=TEMPLATE` | Declare an item the mod registered in Java (repeatable). |
| `--json` (global) | Print the full conversion report as JSON. |

## How a merge works

1. Each source is exported on its own with its own id and namespaces.
2. Files are collected by path. Identical bytes at the same path are kept once
   and counted as `deduplicated`.
3. `data/<source-id>/horizon/...` declarations are rewritten to
   `data/<merge-id>/horizon/...`, because Horizon reads declarations under the
   bundle's own id. Asset and data namespaces are **not** renamed: a texture
   stays at `assets/<source-id>/...`.
4. A path shipped by two sources with different bytes stops the merge, listing
   the paths and their owners. `--keep-first` keeps the first source's copy and
   reports what was dropped.
5. The merged manifest lists every source id, the merged id first in
   `namespaces`, and the union of authors. The bundle is then checked with the
   same rules as `vibe horizon validate`; an invalid result is never written.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Every source converted (and the merge, if requested, is valid). |
| `1` | At least one source failed, the merge was refused, or the output could not be written. |

## Verified with real mods

On Windows, with the Horizon nightly `2ebf00a` (Minecraft 26.3):

```text
Custom Nether Portals 3.0.0 (NeoForge 1.21.1) -> customnetherportals.hmod
WorldEdit 7.3.5 (Forge 1.20.1)                -> worldedit.hmod
both merged                                    -> netherworld.hmod
```

Each bundle passed `vibe horizon validate`. After `vibe horizon install`, a real
Horizon start reported `content from 3 mod(s)` and its content cache listed the
mod ids `netherworld` and the namespaces `customnetherportals`, `netherworld`,
`worldedit`, with no content warnings. The mods' Java code still did not run:
only their resources were loaded.

## See also

- [NATIVE_MOD.md](NATIVE_MOD.md) — running Vibe inside Horizon.
- [COMPATIBILITY.md](COMPATIBILITY.md) — what Vibe can execute in its JVM.
- `vibe horizon validate <hmod>` — the exact checks applied before install.
