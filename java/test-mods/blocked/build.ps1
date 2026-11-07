# Build the blocked-case fixture jars used by Vibe's inspection tests.
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$testMods = Split-Path -Parent $root
$javaRoot = Split-Path -Parent $testMods
$api = Join-Path $javaRoot "build\classes"
$build = Join-Path $testMods "build"

if (-not (Test-Path $api)) {
    throw "Build the Java runtime first: java\build.ps1"
}
if ($env:JAVA_HOME) {
    $javac = Join-Path $env:JAVA_HOME "bin\javac.exe"
} else {
    $javac = (Get-Command javac.exe -ErrorAction Stop).Source
}
$jarTool = Join-Path (Split-Path -Parent $javac) "jar.exe"

$stubs = Join-Path $build "blocked-stubs"
if (Test-Path $stubs) {
    Remove-Item -Recurse -Force $stubs
}
New-Item -ItemType Directory -Force -Path $stubs | Out-Null
$stubSources = Get-ChildItem -Recurse -File -Filter *.java (Join-Path $root "stubs") |
    Select-Object -ExpandProperty FullName
& $javac --release 17 -encoding UTF-8 -Xlint:all -d $stubs $stubSources
if ($LASTEXITCODE -ne 0) {
    throw "javac failed for the blocked-case stubs with exit code $LASTEXITCODE"
}

$plain = Join-Path $root "cases\plain\src"
$cases = @(
    @{ name = "minecraft-ref"; src = (Join-Path $root "cases\minecraft-ref\src"); res = (Join-Path $root "cases\minecraft-ref\resources"); attrs = @() },
    @{ name = "unsupported-forge"; src = (Join-Path $root "cases\unsupported-forge\src"); res = (Join-Path $root "cases\unsupported-forge\resources"); attrs = @() },
    @{ name = "external-lib"; src = (Join-Path $root "cases\external-lib\src"); res = (Join-Path $root "cases\external-lib\resources"); attrs = @() },
    @{ name = "mixin"; src = $plain; res = (Join-Path $root "cases\mixin\resources"); attrs = @("MixinConfigs: mixin_fixture.mixins.json") },
    @{ name = "access-transformer"; src = $plain; res = (Join-Path $root "cases\access-transformer\resources"); attrs = @("FMLAT: META-INF/accesstransformer.cfg") },
    @{ name = "coremod"; src = $plain; res = (Join-Path $root "cases\coremod\resources"); attrs = @("FMLCorePlugin: com.example.blocked.CorePlugin") },
    @{ name = "nested-jar"; src = $plain; res = (Join-Path $root "cases\nested-jar\resources"); attrs = @(); nested = $true },
    @{ name = "no-entrypoint"; src = (Join-Path $root "cases\no-entrypoint\src"); res = (Join-Path $root "cases\no-entrypoint\resources"); attrs = @() },
    @{ name = "unsupported-loader"; src = (Join-Path $root "cases\unsupported-loader\src"); res = (Join-Path $root "cases\unsupported-loader\resources"); attrs = @() }
)

foreach ($case in $cases) {
    $name = $case.name
    $classes = Join-Path $build "blocked-$name\classes"
    New-Item -ItemType Directory -Force -Path $classes | Out-Null
    $sources = Get-ChildItem -Recurse -File -Filter *.java $case.src |
        Select-Object -ExpandProperty FullName
    & $javac --release 17 -encoding UTF-8 -Xlint:all -cp "$api;$stubs" -d $classes $sources
    if ($LASTEXITCODE -ne 0) {
        throw "javac failed for blocked case $name with exit code $LASTEXITCODE"
    }

    $resources = $case.res
    if ($case.nested) {
        $nestedWork = Join-Path $build "blocked-$name\nested-src"
        $nestedStage = Join-Path $build "blocked-$name\resources"
        New-Item -ItemType Directory -Force -Path $nestedWork | Out-Null
        New-Item -ItemType Directory -Force -Path (Join-Path $nestedStage "META-INF\jarjar") | Out-Null
        Set-Content -Path (Join-Path $nestedWork "payload.txt") -Value "nested fixture payload" -NoNewline
        $nestedJar = Join-Path $build "blocked-$name\nested.jar"
        & $jarTool --create --file $nestedJar -C $nestedWork .
        if ($LASTEXITCODE -ne 0) {
            throw "jar failed for the nested fixture payload"
        }
        Copy-Item -Force $nestedJar (Join-Path $nestedStage "META-INF\jarjar\nested.jar")
        Get-ChildItem -Recurse -File $resources | ForEach-Object {
            $relative = $_.FullName.Substring($resources.Length).TrimStart('\')
            $destination = Join-Path $nestedStage $relative
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
            Copy-Item -Force $_.FullName $destination
        }
        $resources = $nestedStage
    }

    $manifest = Join-Path $build "blocked-$name\manifest.mf"
    $lines = @("Manifest-Version: 1.0") + $case.attrs
    Set-Content -Path $manifest -Value $lines -Encoding ASCII

    $jarPath = Join-Path $build "blocked-$name.jar"
    if (Test-Path $jarPath) {
        Remove-Item -Force $jarPath
    }
    & $jarTool --create --file $jarPath --manifest $manifest -C $classes . -C $resources .
    if ($LASTEXITCODE -ne 0) {
        throw "jar failed for blocked case $name with exit code $LASTEXITCODE"
    }
    Write-Host "Built $jarPath"
}
