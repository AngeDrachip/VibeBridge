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

foreach ($name in @("hello-forge", "legacy-hello", "registry-demo")) {
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

# nested-demo bundles a real library under META-INF/jarjar/ so tests can prove
# Vibe extracts nested archives and puts them on the mod class loader.
$nestedModule = Join-Path $root "nested-demo"
$nestedBuild = Join-Path $build "nested-demo"
$libClasses = Join-Path $nestedBuild "lib-classes"
$modClasses = Join-Path $nestedBuild "classes"
$nestedStage = Join-Path $nestedBuild "resources"
New-Item -ItemType Directory -Force -Path $libClasses, $modClasses, (Join-Path $nestedStage "META-INF\jarjar") | Out-Null

$libSources = Get-ChildItem -Recurse -File -Filter *.java (Join-Path $nestedModule "lib") |
    Select-Object -ExpandProperty FullName
& $javac --release 17 -encoding UTF-8 -Xlint:all -d $libClasses $libSources
if ($LASTEXITCODE -ne 0) {
    throw "javac failed for nested-demo library with exit code $LASTEXITCODE"
}
$nestedLibJar = Join-Path $nestedBuild "nested-lib.jar"
& $jarTool --create --file $nestedLibJar -C $libClasses .
if ($LASTEXITCODE -ne 0) {
    throw "jar failed for nested-demo library with exit code $LASTEXITCODE"
}
Copy-Item -Force $nestedLibJar (Join-Path $nestedStage "META-INF\jarjar\nested-lib.jar")
Copy-Item -Recurse -Force (Join-Path $nestedModule "src\main\resources\*") $nestedStage

$modSources = Get-ChildItem -Recurse -File -Filter *.java (Join-Path $nestedModule "src\main\java") |
    Select-Object -ExpandProperty FullName
& $javac --release 17 -encoding UTF-8 -Xlint:all -cp "$api;$libClasses" -d $modClasses $modSources
if ($LASTEXITCODE -ne 0) {
    throw "javac failed for nested-demo with exit code $LASTEXITCODE"
}
$nestedJar = Join-Path $build "nested-demo.jar"
& $jarTool --create --file $nestedJar -C $modClasses . -C $nestedStage .
if ($LASTEXITCODE -ne 0) {
    throw "jar failed for nested-demo with exit code $LASTEXITCODE"
}
Write-Host "Built $nestedJar"
