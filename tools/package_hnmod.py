#!/usr/bin/env python3
"""Package Vibe's native Horizon mod as a source-only `.hnmod`.

The archive holds `horizon-native.json` and `mixins/vibe/` (only `.rs`/`.toml`
and an optional LICENSE), with fixed dates and modes, so the same sources
always give the same bytes and the same SHA-256. Horizon's
`horizon_mods::index::native::Bundle::read` reads it without running anything;
the launcher compiles it on the pinned Horizon revision.
"""
import hashlib
import json
from pathlib import Path
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
MOD = ROOT / "native" / "vibe"
OUT = ROOT / "dist"


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
        "entries": sorted(files),
    }, indent=2))


if __name__ == "__main__":
    package()
