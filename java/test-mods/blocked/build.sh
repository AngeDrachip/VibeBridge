#!/usr/bin/env bash
# Build the blocked-case fixture jars used by Vibe's inspection tests.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
test_mods="$(dirname "$root")"
java_root="$(dirname "$test_mods")"
api="$java_root/build/classes"
build="$test_mods/build"

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

stubs="$build/blocked-stubs"
rm -rf "$stubs"
mkdir -p "$stubs"
mapfile -t stub_sources < <(find "$root/stubs" -name '*.java' -type f | sort)
"$javac" --release 17 -encoding UTF-8 -Xlint:all -d "$stubs" "${stub_sources[@]}"

plain="$root/cases/plain/src"

build_jar() {
  local name="$1"
  local src="$2"
  local res="$3"
  shift 3
  local classes="$build/blocked-$name/classes"
  local manifest="$build/blocked-$name/manifest.mf"
  local jar_path="$build/blocked-$name.jar"
  rm -rf "$classes"
  mkdir -p "$classes"
  mapfile -t sources < <(find "$src" -name '*.java' -type f | sort)
  "$javac" --release 17 -encoding UTF-8 -Xlint:all \
    -cp "$api:$stubs" -d "$classes" "${sources[@]}"
  {
    echo "Manifest-Version: 1.0"
    for attr in "$@"; do
      echo "$attr"
    done
  } > "$manifest"
  rm -f "$jar_path"
  "$jar_tool" --create --file "$jar_path" --manifest "$manifest" \
    -C "$classes" . -C "$res" .
  echo "Built $jar_path"
}

build_jar "minecraft-ref" "$root/cases/minecraft-ref/src" "$root/cases/minecraft-ref/resources"
build_jar "unsupported-forge" "$root/cases/unsupported-forge/src" "$root/cases/unsupported-forge/resources"
build_jar "external-lib" "$root/cases/external-lib/src" "$root/cases/external-lib/resources"
build_jar "mixin" "$plain" "$root/cases/mixin/resources" "MixinConfigs: mixin_fixture.mixins.json"
build_jar "access-transformer" "$plain" "$root/cases/access-transformer/resources" "FMLAT: META-INF/accesstransformer.cfg"
build_jar "coremod" "$plain" "$root/cases/coremod/resources" "FMLCorePlugin: com.example.blocked.CorePlugin"

nested_work="$build/blocked-nested-jar/nested-src"
nested_stage="$build/blocked-nested-jar/resources"
rm -rf "$nested_work" "$nested_stage"
mkdir -p "$nested_work" "$nested_stage/META-INF/jarjar"
printf 'nested fixture payload' > "$nested_work/payload.txt"
"$jar_tool" --create --file "$build/blocked-nested-jar/nested.jar" -C "$nested_work" .
cp "$build/blocked-nested-jar/nested.jar" "$nested_stage/META-INF/jarjar/nested.jar"
cp -R "$root/cases/nested-jar/resources/." "$nested_stage/"
build_jar "nested-jar" "$plain" "$nested_stage"

build_jar "no-entrypoint" "$root/cases/no-entrypoint/src" "$root/cases/no-entrypoint/resources"
build_jar "unsupported-loader" "$root/cases/unsupported-loader/src" "$root/cases/unsupported-loader/resources"
