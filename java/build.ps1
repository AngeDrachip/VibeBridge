# Build Vibe's Java runtime and clean-room Forge API subset.
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$build = Join-Path $root "build"
$classes = Join-Path $build "classes"
$jar = Join-Path $build "vibe-runtime.jar"

if ($env:JAVA_HOME) {
    $javac = Join-Path $env:JAVA_HOME "bin\javac.exe"
} else {
    $javac = (Get-Command javac.exe -ErrorAction Stop).Source
}
if (-not (Test-Path $javac)) {
    throw "javac not found. Install a JDK 17+ or set JAVA_HOME."
}
$jdkBin = Split-Path -Parent $javac
$jarTool = Join-Path $jdkBin "jar.exe"

$versionText = (& $javac -version 2>&1 | Out-String).Trim()
if ($versionText -notmatch "javac\s+(\d+)") {
    throw "Could not read javac version from: $versionText"
}
$major = [int]$Matches[1]
if ($major -lt 17) {
    throw "Vibe needs JDK 17 or newer; found $versionText"
}

if (Test-Path $classes) {
    Remove-Item -Recurse -Force $classes
}
New-Item -ItemType Directory -Force -Path $classes | Out-Null

$sources = @()
$sources += Get-ChildItem -Recurse -File -Filter *.java (Join-Path $root "vibe-forge-api\src\main\java") |
    Select-Object -ExpandProperty FullName
$sources += Get-ChildItem -Recurse -File -Filter *.java (Join-Path $root "vibe-runtime\src\main\java") |
    Select-Object -ExpandProperty FullName

& $javac --release 17 -encoding UTF-8 -Xlint:all -d $classes $sources
if ($LASTEXITCODE -ne 0) {
    throw "javac failed with exit code $LASTEXITCODE"
}

if (Test-Path $jar) {
    Remove-Item -Force $jar
}
& $jarTool --create --file $jar --main-class dev.vibe.runtime.VibeRuntime -C $classes .
if ($LASTEXITCODE -ne 0) {
    throw "jar failed with exit code $LASTEXITCODE"
}

Write-Host "Built $jar"
Write-Host "Classes: $classes"
