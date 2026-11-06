# Build the integration-test mod jars against Vibe's clean-room Forge API.
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$javaRoot = Split-Path -Parent $root
$api = Join-Path $javaRoot "build\classes"
$build = Join-Path $root "build"

if (-not (Test-Path $api)) {
    throw "Build the Java runtime first: java\build.ps1"
}
if ($env:JAVA_HOME) {
    $javac = Join-Path $env:JAVA_HOME "bin\javac.exe"
} else {
    $javac = (Get-Command javac.exe -ErrorAction Stop).Source
}
$jarTool = Join-Path (Split-Path -Parent $javac) "jar.exe"

if (Test-Path $build) {
    Remove-Item -Recurse -Force $build
}
New-Item -ItemType Directory -Force -Path $build | Out-Null

foreach ($name in @("hello-forge", "legacy-hello")) {
    $module = Join-Path $root $name
    $classes = Join-Path $build "$name\classes"
    New-Item -ItemType Directory -Force -Path $classes | Out-Null
    $sources = Get-ChildItem -Recurse -File -Filter *.java (Join-Path $module "src\main\java") |
        Select-Object -ExpandProperty FullName
    & $javac --release 17 -encoding UTF-8 -Xlint:all -cp $api -d $classes $sources
    if ($LASTEXITCODE -ne 0) {
        throw "javac failed for $name with exit code $LASTEXITCODE"
    }
    $jarPath = Join-Path $build "$name.jar"
    & $jarTool --create --file $jarPath -C $classes . -C (Join-Path $module "src\main\resources") .
    if ($LASTEXITCODE -ne 0) {
        throw "jar failed for $name with exit code $LASTEXITCODE"
    }
    Write-Host "Built $jarPath"
}
