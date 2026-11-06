#!/usr/bin/env bash
# Build Vibe's Java runtime and clean-room Forge API subset.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
build="$root/build"
classes="$build/classes"
jar="$build/vibe-runtime.jar"

if [[ -n "${JAVA_HOME:-}" ]]; then
  javac="$JAVA_HOME/bin/javac"
else
  javac="$(command -v javac)"
fi
if [[ ! -x "$javac" ]]; then
  echo "javac not found. Install a JDK 17+ or set JAVA_HOME." >&2
  exit 1
fi
jar_tool="$(dirname "$javac")/jar"

version_text="$("$javac" -version 2>&1)"
major="$(sed -n 's/^javac \([0-9][0-9]*\).*/\1/p' <<<"$version_text")"
if [[ -z "$major" || "$major" -lt 17 ]]; then
  echo "Vibe needs JDK 17 or newer; found: $version_text" >&2
  exit 1
fi

rm -rf "$classes"
mkdir -p "$classes"

mapfile -t sources < <(find "$root/vibe-forge-api/src/main/java" "$root/vibe-runtime/src/main/java" -name '*.java' -type f)
"$javac" --release 17 -encoding UTF-8 -Xlint:all -d "$classes" "${sources[@]}"

rm -f "$jar"
"$jar_tool" --create --file "$jar" --main-class dev.vibe.runtime.VibeRuntime -C "$classes" .

echo "Built $jar"
echo "Classes: $classes"
