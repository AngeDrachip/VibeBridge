#!/usr/bin/env python3
"""Package the Windows release zip, including the optimized JRE.

The archive is the whole install: `vibe.exe`, `java/build/vibe-runtime.jar`,
the jlink image at `java/runtime/`, docs, and the `mods/` placeholder. Every
entry gets a fixed timestamp and mode, and entries are written in sorted order,
so the same inputs always produce the same bytes.

    python tools/package_release.py [--skip-runtime-check]

`java/runtime/` is built by `java/build-runtime.ps1` (or `build-runtime.sh`).
"""
from __future__ import annotations

import argparse
import hashlib
import re
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
EXE = ROOT / "target" / "release" / "vibe.exe"
JAR = ROOT / "java" / "build" / "vibe-runtime.jar"
RUNTIME = ROOT / "java" / "runtime"
DATE = (2026, 1, 1, 0, 0, 0)
PLACEHOLDER = """Drop Forge mod jars in this folder, then run:
  vibe run mods

Vibe inspects them read-only and runs the Forge API subset it supports.

Horizon content bundles (.hmod) do not go here: install them with
  vibe horizon install <file>.hmod
"""
DOCS = (
    "README.md",
    "INSTALL.md",
    "CHANGELOG.md",
    "1.0.0-BETA1-RELEASE-NOTES.md",
    "docs/CONVERT.md",
)


def version() -> str:
    text = (ROOT / "Cargo.toml").read_text()
    match = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
    if not match:
        raise SystemExit("no workspace version in Cargo.toml")
    return match.group(1)


def java_binary() -> Path:
    return RUNTIME / "bin" / "java.exe"


def check_inputs(skip_runtime: bool) -> None:
    if not EXE.is_file():
        raise SystemExit(f"missing {EXE}; run: cargo build --release -p vibe-cli")
    if not JAR.is_file():
        raise SystemExit(f"missing {JAR}; run: java/build.ps1")
    if skip_runtime:
        return
    if not java_binary().is_file():
        raise SystemExit(f"missing {java_binary()}; run: java/build-runtime.ps1")
    modules = RUNTIME / "lib" / "modules"
    if not modules.is_file():
        raise SystemExit(f"{modules} is missing; {RUNTIME} is not a jlink image")
    cds = RUNTIME / "bin" / "server" / "classes.jsa"
    if not cds.is_file():
        raise SystemExit(f"{cds} is missing; rebuild with --generate-cds-archive")


def runtime_files() -> list[Path]:
    """The jlink image without the CDS variants Vibe never uses or link stubs."""
    files = []
    for path in sorted(RUNTIME.rglob("*")):
        if not path.is_file():
            continue
        name = path.name
        if re.fullmatch(r"classes_(nocoops|nocoh|nocoops_nocoh)\.jsa", name):
            continue
        if path.suffix == ".lib":
            continue
        files.append(path)
    return files


def add(archive: zipfile.ZipFile, name: str, data: bytes) -> None:
    info = zipfile.ZipInfo(name, DATE)
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o100644 << 16
    archive.writestr(info, data)


def package(skip_runtime: bool) -> Path:
    check_inputs(skip_runtime)
    ver = version()
    DIST.mkdir(exist_ok=True)
    out = DIST / f"vibe-{ver}-windows-x86_64.zip"

    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        add(archive, "vibe.exe", EXE.read_bytes())
        add(archive, "java/build/vibe-runtime.jar", JAR.read_bytes())
        if not skip_runtime:
            for path in runtime_files():
                add(archive, path.relative_to(ROOT).as_posix(), path.read_bytes())
        for doc in DOCS:
            path = ROOT / doc
            if path.is_file():
                add(archive, doc, path.read_bytes())
        add(archive, "mods/PUT-FORGE-JARS-HERE.txt", PLACEHOLDER.encode())

    digest = hashlib.sha256(out.read_bytes()).hexdigest()
    runtime_bytes = sum(p.stat().st_size for p in runtime_files()) if not skip_runtime else 0
    print(f"{out.name}")
    print(f"  size    {out.stat().st_size / 1e6:.1f} MB")
    print(f"  runtime {runtime_bytes / 1e6:.1f} MB uncompressed")
    print(f"  sha256  {digest}")
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--skip-runtime-check",
        action="store_true",
        help="package without java/runtime (development only)",
    )
    args = parser.parse_args()
    package(args.skip_runtime_check)


if __name__ == "__main__":
    main()
