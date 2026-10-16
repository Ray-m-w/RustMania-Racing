# Play the Rust + Bevy port: the game's title and menus (Session 7: main
# menu, SELECT COURSE, the course menu, character / vehicle select, pause
# menu, options), then a 3-lap 1 PLAYER RACE against five AI opponents (four
# on SPEEDWAY / RALLY / GLACIER) to the results; START there goes back to
# the course menu.
#
#   .\tools\play-port.ps1            # build (release) and play from the title
#   .\tools\play-port.ps1 -Course BOG   # that course's menu (DOCK, BOG,
#                                    # NEW_YORK, ... or its index)
#   .\tools\play-port.ps1 -Course BOG -Race   # straight into its race (Kermit)
#   .\tools\play-port.ps1 -Demo      # the game's AI drives Kermit too
#   .\tools\play-port.ps1 -Record    # solo lap aligned to the oracle savestate,
#                                    # recorded for tools\compare-drive.ps1
#
#   .\tools\play-port.ps1 -Hz30      # the game's 30 Hz logic (60 Hz is the default)
#   .\tools\play-port.ps1 --msaa 8   # any other mr-bevy option or --set key=value
#
# Controls (keyboard): arrows steer, X accelerate (CROSS), S brake/reverse
# (CIRCLE), Z jump (SQUARE), A = TRIANGLE (camera lock-on), Enter = START
# (pause). Menus: arrows move, X = CROSS selects, A = TRIANGLE goes back.
# Gamepad: south = accelerate, east = brake, west = jump, right trigger = accelerate, left trigger = brake, D-pad/left stick steer.
# Items (Session 4): Q = L1 / W = R1 (turbo; special move on a full meter) /
# E = L2 / R = R2 (character special); pad: L1/R1 bumpers, L2/R2 = stick clicks.
# Sound and music are on; see port/README.md for --music-volume, --no-audio etc.
# The disc: settings.txt `disc` (data folder), $env:MRM_DISC or game.toml; nothing from
# the recomp is used at run time.
#
# -Record writes analysis\drives\<stamp>.route.json (the inputs, in the oracle's
# route format, aligned to the oracle's DOCK grid savestate) and
# <stamp>.bevy.trace.jsonl (the Rust run). Compare with
# .\tools\compare-drive.ps1 <stamp>.
param([switch]$Record, [switch]$Demo, [switch]$Leader, [switch]$Hz30, [string]$Course, [switch]$Race,
      [Parameter(ValueFromRemainingArguments = $true)][string[]]$Rest)
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
if ($Hz30) { $argv += @("--set", "logic_hz=30") }
if ($Course) { $argv += @("--course", $Course, "--skip-title"); if (-not $Race) { $argv += "--menu" } }
if ($Record) {
    $dir = Join-Path $Root "analysis\drives"
    New-Item -ItemType Directory -Force $dir | Out-Null
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $argv += @("--oracle-grid", "--record-route", (Join-Path $dir "$stamp.route.json"),
               "--trace-out", (Join-Path $dir "$stamp.bevy.trace.jsonl"))
    "recording drive $stamp (close the window to finish)"
}
if ($Rest) { $argv += $Rest }
Push-Location $Root
try { & $exe @argv } finally { Pop-Location }
