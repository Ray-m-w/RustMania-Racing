# Play the Rust + Bevy port: a 3-lap 1-PLAYER RACE on DOCK, Kermit in the
# RACER against five AI opponents, from race load (countdown and the
# "Copy the buttons" start game) to the results. START on the results restarts.
#
#   .\tools\play-port.ps1            # build (release) and play
#   .\tools\play-port.ps1 -Demo      # the game's AI drives Kermit too
#   .\tools\play-port.ps1 -Record    # solo lap aligned to the oracle savestate,
#                                    # recorded for tools\compare-drive.ps1
#
# Controls (keyboard): arrows steer, X accelerate (CROSS), S brake/reverse
# (CIRCLE), Z jump (SQUARE), A = TRIANGLE (start game), Enter = START. Gamepad: south = accelerate, east = brake, west =
# jump, right trigger = accelerate, left trigger = brake, D-pad/left stick steer.
# Items (Session 4): Q = L1 / W = R1 (turbo; special move on a full meter) /
# E = L2 / R = R2 (character special); pad: L1/R1 bumpers, L2/R2 = stick clicks.
# Sound and music are on; see port/README.md for --music-volume, --no-audio etc.
# The disc is read from game.toml's disc line (or $env:MRM_DISC); nothing from
# the recomp is used at run time.
#
# -Record writes analysis\drives\<stamp>.route.json (the inputs, in the oracle's
# route format, aligned to the oracle's DOCK grid savestate) and
# <stamp>.bevy.trace.jsonl (the Rust run). Compare with
# .\tools\compare-drive.ps1 <stamp>.
param([switch]$Record, [switch]$Demo, [switch]$Leader)
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
if ($Demo) { $argv += "--autopilot" }
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
