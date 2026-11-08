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

for name in hello-forge legacy-hello registry-demo; do
  module="$root/$name"
  classes="$build/$name/classes"
  mkdir -p "$classes"
  mapfile -t sources < <(find "$module/src/main/java" -name '*.java' -type f)
  "$javac" --release 17 -encoding UTF-8 -Xlint:all -cp "$api" -d "$classes" "${sources[@]}"
  "$jar_tool" --create --file "$build/$name.jar" -C "$classes" . -C "$module/src/main/resources" .
  echo "Built $build/$name.jar"
done

# nested-demo bundles a real library under META-INF/jarjar/.
nested_module="$root/nested-demo"
nested_build="$build/nested-demo"
lib_classes="$nested_build/lib-classes"
mod_classes="$nested_build/classes"
nested_stage="$nested_build/resources"
mkdir -p "$lib_classes" "$mod_classes" "$nested_stage/META-INF/jarjar"

mapfile -t lib_sources < <(find "$nested_module/lib" -name '*.java' -type f)
"$javac" --release 17 -encoding UTF-8 -Xlint:all -d "$lib_classes" "${lib_sources[@]}"
"$jar_tool" --create --file "$nested_build/nested-lib.jar" -C "$lib_classes" .
cp "$nested_build/nested-lib.jar" "$nested_stage/META-INF/jarjar/nested-lib.jar"
cp -R "$nested_module/src/main/resources/." "$nested_stage/"

mapfile -t mod_sources < <(find "$nested_module/src/main/java" -name '*.java' -type f)
"$javac" --release 17 -encoding UTF-8 -Xlint:all -cp "$api:$lib_classes" -d "$mod_classes" "${mod_sources[@]}"
"$jar_tool" --create --file "$build/nested-demo.jar" -C "$mod_classes" . -C "$nested_stage" .
echo "Built $build/nested-demo.jar"
