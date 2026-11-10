#!/usr/bin/env bash
# Build the optimized JRE that ships beside vibe: java/runtime.
#
# jlink keeps only the modules Vibe and Forge mods need, strips debug attributes
# and (JDK 19+) generates a CDS archive so the JVM maps its class library at
# startup. The result is ~45 MB instead of a ~200 MB JDK and starts in ~80 ms
# instead of ~130 ms without CDS.
#
#   java/build-runtime.sh
#
# Needs a JDK (not a JRE): jlink and the jmods. `VIBE_RUNTIME_JDK` picks one.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out="$root/runtime"

find_jdk() {
    if [[ -n "${VIBE_RUNTIME_JDK:-}" && -x "$VIBE_RUNTIME_JDK/bin/jlink" ]]; then
        printf '%s\n' "$VIBE_RUNTIME_JDK"
        return
    fi
    local candidates=()
    if [[ -n "${JAVA_HOME:-}" && -x "$JAVA_HOME/bin/jlink" ]]; then
        candidates+=("$JAVA_HOME")
    fi
    for dir in /usr/lib/jvm /Library/Java/JavaVirtualMachines /opt/homebrew/opt /opt/java; do
        [[ -d "$dir" ]] || continue
        for candidate in "$dir"/*/bin/jlink "$dir"/*/*/bin/jlink; do
            [[ -x "$candidate" ]] && candidates+=("$(dirname "$(dirname "$candidate")")")
        done
    done
    if command -v jlink >/dev/null 2>&1; then
        candidates+=("$(dirname "$(dirname "$(command -v jlink)")")")
    fi
    if [[ ${#candidates[@]} -eq 0 ]]; then
        echo "No JDK with jlink found. Install a JDK 17+ or set JAVA_HOME." >&2
        exit 1
    fi
    # Newest major first: its JVM has CDS (`--generate-cds-archive`).
    local best="" best_major=-1 major
    for candidate in "${candidates[@]}"; do
        major="$(basename "$candidate" | grep -oE '[0-9]+' | head -1)"
        major="${major:-0}"
        if (( major > best_major )); then
            best="$candidate"
            best_major="$major"
        fi
    done
    printf '%s\n' "$best"
}

jdk="$(find_jdk)"
jlink="$jdk/bin/jlink"
if [[ ! -d "$jdk/jmods" ]]; then
    echo "$jdk has no jmods folder; point JAVA_HOME at a JDK, not a JRE." >&2
    exit 1
fi

version="$("$jlink" --version)"
major="$(printf '%s' "$version" | grep -oE '[0-9]+' | head -1)"
if [[ -z "$major" || "$major" -lt 17 ]]; then
    echo "jlink $version is too old; Vibe needs JDK 17 or newer." >&2
    exit 1
fi

# Vibe itself only needs java.base, but Forge mods and their libraries reach
# for logging, XML, naming, JMX, instrumentation, HTTP and Unsafe. desktop/sql
# are left out on purpose: the sidecar is server-side and they are the bulk of
# a full JDK.
modules="java.base,java.logging,java.xml,java.naming,java.management,java.instrument,java.net.http,jdk.unsupported,jdk.zipfs,jdk.crypto.ec"

rm -rf "$out"

args=(--add-modules "$modules" --strip-debug --no-header-files --no-man-pages --output "$out")
if [[ "$major" -ge 21 ]]; then
    args+=(--compress=zip-6)
else
    args+=(--compress=2)
fi
if [[ "$major" -ge 19 ]]; then
    args+=(--generate-cds-archive)
fi

echo "jlink $version -> $out"
"$jlink" "${args[@]}"

# jlink writes four CDS variants; Vibe's heaps keep compressed oops on, so only
# classes.jsa is used and the other three are ~42 MB of dead weight. The static
# link library is for embedding, not for running.
if [[ -d "$out/bin/server" ]]; then
    find "$out/bin/server" -maxdepth 1 -name 'classes_*.jsa' -delete
fi
find "$out/lib" -maxdepth 1 -name '*.lib' -delete 2>/dev/null || true

bytes="$(find "$out" -type f -printf '%s\n' | awk '{ total += $1 } END { print total + 0 }')"
awk -v b="$bytes" -v o="$out" 'BEGIN { printf "Runtime: %.1f MB at %s\n", b / 1000000, o }'
"$out/bin/java" -version 2>&1 | head -3 || true
