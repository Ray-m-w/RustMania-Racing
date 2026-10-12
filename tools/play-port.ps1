# Play the Rust + Bevy port (milestone 1: Kermit in the RACER on DOCK).
#
#   .\tools\play-port.ps1            # build (release) and play
#   .\tools\play-port.ps1 -Record    # also record the drive for the oracle comparison
#   .\tools\play-port.ps1 -Leader    # catch-up off (no AI: the player leads)
#
# Controls (keyboard): arrows steer, X accelerate (CROSS), S brake/reverse
# (CIRCLE), Z jump (SQUARE). Gamepad: south = accelerate, east = brake, west =
# jump, right trigger = accelerate, left trigger = brake, D-pad/left stick steer.
# The disc is read from game.toml's disc line (or $env:MRM_DISC); nothing from
# the recomp is used at run time.
#
# -Record writes analysis\drives\<stamp>.route.json (the inputs, in the oracle's
# route format, aligned to the oracle's DOCK grid savestate) and
# <stamp>.bevy.trace.jsonl (the Rust run). Compare with
# .\tools\compare-drive.ps1 <stamp>.
param([switch]$Record, [switch]$Leader)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Push-Location (Join-Path $Root "port")
try {
    cargo build --release --offline -p mr-bevy
    if ($LASTEXITCODE -ne 0) { throw "build failed" }
} finally { Pop-Location }
$exe = Join-Path $Root "port\target\release\mr-bevy.exe"
$argv = @()
if ($Leader) { $argv += "--leader" }
if ($Record) {
    $dir = Join-Path $Root "analysis\drives"
    New-Item -ItemType Directory -Force $dir | Out-Null
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $argv += @("--oracle-grid", "--record-route", (Join-Path $dir "$stamp.route.json"),
               "--trace-out", (Join-Path $dir "$stamp.bevy.trace.jsonl"))
    "recording drive $stamp (close the window to finish)"
}
Push-Location $Root
try { & $exe @argv } finally { Pop-Location }
