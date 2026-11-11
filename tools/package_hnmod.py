#!/usr/bin/env python3
"""Package Vibe's native Horizon mod as a source-only `.hnmod`.

The archive holds `horizon-native.json`, `mixins/vibe/` (only `.rs`/`.toml`
and an optional LICENSE), and `mixins/vibe/data/jre.zip`: the optimized JRE
the game extracts on first start. Horizon lets a bundle carry binary files
under `data/`, and the launcher compiles them into the game with
`native_data!`, so the `.hnmod` needs no Java on the player's machine.

Every entry has fixed dates and modes, and the inner JRE zip is built the same
way each time, so the same sources always give the same bytes and the same
SHA-256. Horizon's `horizon_mods::index::native::Bundle::read` reads it without
running anything; the launcher compiles it on the pinned Horizon revision.
"""
import hashlib
import io
import json
from pathlib import Path
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
MOD = ROOT / "native" / "vibe"
OUT = ROOT / "dist"
RUNTIME = ROOT / "java" / "runtime"


def jre_zip() -> bytes:
    """The JRE as one deterministic zip, extracted to `vibe/java/runtime`.

    The launcher stages it as `native-data/vibe/jre.zip` and the game compiles
    it in; `runtime.rs` unpacks it on first start. Sorted entries and fixed
    metadata keep the bytes stable, so the release hash means something.
    """
    exe = RUNTIME / "bin" / ("java.exe" if (RUNTIME / "bin" / "java.exe").is_file() else "java")
    if not exe.is_file():
        raise SystemExit(
            f"{RUNTIME} has no Java runtime: run java/build-runtime.ps1 (or .sh) first; "
            "the .hnmod carries the JRE so players need no Java"
        )
    files = sorted(p for p in RUNTIME.rglob("*") if p.is_file())
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in files:
            name = path.relative_to(RUNTIME).as_posix()
            info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (0o100755 if name.startswith("bin/") else 0o100644) << 16
            archive.writestr(info, path.read_bytes())
    data = buffer.getvalue()
    if len(files) > 512:
        raise SystemExit(f"the JRE has {len(files)} files: a bundle takes 512 at most")
    return data


def package() -> None:
    compat = json.loads((MOD / "compatibility.json").read_text())
    manifests = sorted((MOD / "mixins").glob("*/horizon-mixin.toml"))
    if len(manifests) != 1:
        raise SystemExit("a .hnmod holds one bundle: keep one mixins/<id>/horizon-mixin.toml")
    bundle = manifests[0].parent
    mixin = tomllib.loads(manifests[0].read_text())
    mod = mixin["mod"]
    if bundle.name != mod["id"]:
        raise SystemExit(f"rename mixins/{bundle.name}/ to mixins/{mod['id']}/ (the registry finds it by id)")
    if not mod["id"].replace("_", "").isalnum() or not mod["id"].islower():
        raise SystemExit("the registry takes ids of lowercase letters, digits and _ only")
    pinned = mixin.get("compatibility", {}).get("horizon_revision")
    if pinned != compat["horizon_revision"]:
        raise SystemExit("horizon-mixin.toml and compatibility.json pin different Horizon revisions")
    manifest = {
        "schema": 1,
        "id": mod["id"],
        "version": mod["version"],
        "game": compat["game"],
        "horizon": compat["horizon_revision"],
        "mixins": compat["horizon_mixins_revision"],
        "hooks": compat["hooks"],
    }
    files = {"horizon-native.json": (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()}
    for path in sorted(bundle.rglob("*")):
        if path.is_file() and path.suffix in (".rs", ".toml"):
            files[path.relative_to(MOD).as_posix()] = path.read_bytes()
    licence = ROOT / "LICENSE"
    if licence.is_file():
        files[f"mixins/{mod['id']}/LICENSE"] = licence.read_bytes()
    runtime = jre_zip()
    files[f"mixins/{mod['id']}/data/jre.zip"] = runtime
    OUT.mkdir(exist_ok=True)
    out = OUT / f"{mod['id']}-{mod['version']}.hnmod"
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, content in sorted(files.items()):
            info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, content)
    print(json.dumps({
        "file": str(out),
        "size": out.stat().st_size,
        "sha256": hashlib.sha256(out.read_bytes()).hexdigest(),
        "jre_zip_size": len(runtime),
        "entries": sorted(files),
    }, indent=2))



if __name__ == "__main__":
    package()
