#!/usr/bin/env bash
# Build the integration-test mod jars against Vibe's clean-room Forge API.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
java_root="$(dirname "$root")"
api="$java_root/build/classes"
build="$root/build"

if [[ ! -d "$api" ]]; then
  echo "Build the Java runtime first: java/build.sh" >&2
  exit 1
fi
if [[ -n "${JAVA_HOME:-}" ]]; then
  javac="$JAVA_HOME/bin/javac"
else
  javac="$(command -v javac)"
fi
jar_tool="$(dirname "$javac")/jar"

rm -rf "$build"
mkdir -p "$build"

for name in hello-forge legacy-hello; do
  module="$root/$name"
  classes="$build/$name/classes"
  mkdir -p "$classes"
  mapfile -t sources < <(find "$module/src/main/java" -name '*.java' -type f)
  "$javac" --release 17 -encoding UTF-8 -Xlint:all -cp "$api" -d "$classes" "${sources[@]}"
  "$jar_tool" --create --file "$build/$name.jar" -C "$classes" . -C "$module/src/main/resources" .
  echo "Built $build/$name.jar"
done
