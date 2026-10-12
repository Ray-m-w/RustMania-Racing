# One-command build: from a clean checkout plus your own disc to a playable,
# portable Windows build.
#
#   .\tools\build.ps1 -Disc "F:\path\to\Muppet RaceMania (USA).cue" [-Bios F:\BIOS\scph1001.bin] [-Package]
#
# Steps (the commands psxrecomp documents; see docs/notes/2026-10-11.md):
#   1. psxrecomp/tools/ci/build_emitters.sh          (recompiler tools)
#   2. psxrecomp_cli.py generate                      (game C from YOUR disc -> generated/, gitignored)
#   3. cmake configure + build, Release               (build-release/)
#   4. -Package: scripts/package_release.sh           (dist/mrm-<ver>-windows-x64.zip)
#
# Needs: Git for Windows, and psxrecomp's cmake-clang-v1 toolchain pack
# (see tools/devshell.ps1). No game data is downloaded; the disc is yours.
param(
    [Parameter(Mandatory = $true)][string]$Disc,
    [string]$Bios = "",
    [switch]$Package
)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "devshell.ps1")
Set-Location $Root

function Run([string]$what, [scriptblock]$cmd) {
    Write-Host "== $what"
    & $cmd
    if ($LASTEXITCODE -ne 0) { throw "$what failed (exit $LASTEXITCODE)" }
}

if (-not (Test-Path (Join-Path $Root "psxrecomp\runtime\runtime.cmake"))) {
    Run "submodules" { git submodule update --init --recursive }
}
Run "emitters" { bash psxrecomp/tools/ci/build_emitters.sh }
$gen = @("psxrecomp\psxrecomp_cli.py", "generate", "--config", "game.toml",
         "--project-root", ".", "--disc", $Disc)
if ($Bios) { $gen += @("--bios", $Bios) }
Run "generate game C" { python @gen }
Run "configure" { cmake -S . -B build-release -G Ninja -DCMAKE_BUILD_TYPE=Release }
Run "build" { cmake --build build-release --target psx-runtime }
Write-Host "Built: $(Join-Path $Root 'build-release')"

if ($Package) {
    $env:RELEASE_VERSION = (Get-Content (Join-Path $Root "VERSION")).Trim()
    $env:PSX_RELEASE_STAGE_PYTHON = $env:RETCOMM_PYTHON
    if (Test-Path (Join-Path $Root "build-dev\cache")) { $env:PSX_OVERLAY_CACHE_ROOT = "build-dev/cache" }
    elseif (Test-Path (Join-Path $Root "build-release\cache")) { $env:PSX_OVERLAY_CACHE_ROOT = "build-release/cache" }
    Run "package" { bash scripts/package_release.sh build-release windows-x64 build-recompiler }
}
