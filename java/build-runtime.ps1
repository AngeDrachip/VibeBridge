# Build the optimized JRE that ships beside vibe.exe: java/runtime.
#
# jlink keeps only the modules Vibe and Forge mods need, strips debug attributes
# and generates a CDS archive so the JVM starts with its class library already
# mapped. The result is ~45 MB instead of a ~200 MB JDK, and starts in ~78 ms
# instead of ~130 ms without CDS.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File java\build-runtime.ps1
#
# Needs a JDK (not a JRE): jlink and the jmods. JDK 21+ gives the smaller
# `--compress=zip-6` and `--generate-cds-archive`; JDK 17 works too.
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$out = Join-Path $root "runtime"

function Find-Jdk {
    if ($env:VIBE_RUNTIME_JDK -and (Test-Path (Join-Path $env:VIBE_RUNTIME_JDK "bin\jlink.exe"))) {
        return $env:VIBE_RUNTIME_JDK
    }
    $candidates = @()
    if ($env:JAVA_HOME -and (Test-Path (Join-Path $env:JAVA_HOME "bin\jlink.exe"))) {
        $candidates += $env:JAVA_HOME
    }
    $roots = @(
        "C:\Program Files\Java",
        "C:\Program Files\Eclipse Adoptium",
        "C:\Program Files\Microsoft",
        "C:\Program Files\Zulu"
    )
    foreach ($dir in $roots) {
        if (Test-Path $dir) {
            $candidates += Get-ChildItem $dir -Directory |
                Where-Object { Test-Path (Join-Path $_.FullName "bin\jlink.exe") } |
                Select-Object -ExpandProperty FullName
        }
    }
    if ($candidates.Count -gt 0) {
        # The newest JDK: its JVM runs the Java 17 runtime jar and has CDS
        # (`--generate-cds-archive`, JDK 19+), which cuts startup by ~40%.
        $newest = $candidates | Sort-Object {
            [int]([regex]::Match((Split-Path $_ -Leaf), '\d+').Value)
        } -Descending | Select-Object -First 1
        return $newest
    }
    $jlink = Get-Command jlink.exe -ErrorAction SilentlyContinue
    if ($jlink) {
        return (Split-Path -Parent (Split-Path -Parent $jlink.Source))
    }
    throw "No JDK with jlink found. Install a JDK 17+ or set JAVA_HOME."
}

$jdk = Find-Jdk
$jlink = Join-Path $jdk "bin\jlink.exe"
$jmods = Join-Path $jdk "jmods"
if (-not (Test-Path $jmods)) {
    throw "$jdk has no jmods folder; point JAVA_HOME at a JDK, not a JRE."
}

$versionText = (& $jlink --version 2>&1 | Out-String).Trim()
$major = 0
if ($versionText -match "(\d+)") { $major = [int]$Matches[1] }
if ($major -lt 17) {
    throw "jlink $versionText is too old; Vibe needs JDK 17 or newer."
}

# Vibe itself only needs java.base, but Forge mods and their libraries reach
# for logging, XML, naming, JMX, instrumentation, HTTP and Unsafe. desktop/sql
# are left out on purpose: Vibe's sidecar is server-side, and they are the bulk
# of a full JDK.
$modules = @(
    "java.base",
    "java.logging",
    "java.xml",
    "java.naming",
    "java.management",
    "java.instrument",
    "java.net.http",
    "jdk.unsupported",
    "jdk.zipfs",
    "jdk.crypto.ec"
) -join ","

if (Test-Path $out) {
    Remove-Item $out -Recurse -Force
}

$arguments = @(
    "--add-modules", $modules,
    "--strip-debug",
    "--no-header-files",
    "--no-man-pages",
    "--output", $out
)
if ($major -ge 21) {
    $arguments += @("--compress=zip-6")
} else {
    $arguments += @("--compress=2")
}
if ($major -ge 19) {
    $arguments += @("--generate-cds-archive")
}

Write-Output "jlink $versionText -> $out"
& $jlink @arguments
if ($LASTEXITCODE -ne 0) {
    throw "jlink failed with exit code $LASTEXITCODE"
}

# jlink writes four CDS variants (with/without compressed oops and class
# pointers). Vibe's heaps keep compressed oops on, so only classes.jsa is used;
# the other three are ~42 MB of dead weight.
$server = Join-Path $out "bin\server"
if (Test-Path $server) {
    Get-ChildItem $server -Filter "classes_*.jsa" | Remove-Item -Force
}
# The static link library is for embedding, not for running.
Get-ChildItem (Join-Path $out "lib") -Filter "*.lib" -ErrorAction SilentlyContinue | Remove-Item -Force

$bytes = (Get-ChildItem $out -Recurse -File | Measure-Object Length -Sum).Sum
Write-Output ("Runtime: {0:N1} MB at {1}" -f ($bytes / 1MB), $out)
$ErrorActionPreference = "Continue"
$versionLine = (& (Join-Path $out "bin\java.exe") -version 2>&1 | Out-String).Trim()
$ErrorActionPreference = "Stop"
Write-Output $versionLine
