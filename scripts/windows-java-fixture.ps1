$ErrorActionPreference = 'Stop'
$project = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $project
try {
    $runtime = (Get-Command java.exe -CommandType Application | Select-Object -First 1).Source
    $compiler = Join-Path (Split-Path $runtime) 'javac.exe'
    $classes = New-Item -Force -ItemType Directory '.artifacts/java-probe'
    # This is compilation of the repository-owned observation helper during development.
    & $compiler --release 17 -proc:none -classpath $classes.FullName -d $classes.FullName 'crates/isolation-probe/FixtureProcessor.java'
    if ($LASTEXITCODE -ne 0) { throw 'Could not compile the owned observation helper.' }
    cargo run --locked -p carbon-isolation-probe -- --java-fixture $runtime
    if ($LASTEXITCODE -ne 0) { throw 'The contained Java fixture failed.' }
} finally { Pop-Location }
