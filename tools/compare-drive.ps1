# Replay a drive recorded in the Rust port on the recomp (the oracle) and
# compare the two runs tick by tick.
#
#   .\tools\compare-drive.ps1 <stamp>      # from analysis\drives\<stamp>.*
#
# Needs the debug build running with the oracle bridge:
#   .\tools\run-dev.ps1 -Start   (dev package mrm.dev.oracle-bridge enabled in
#   build-dev\mods\state.toml, savestate slot 2 = DOCK grid; see
#   docs\history\notes\SESSION-2-oracle.md)
# Writes <stamp>.oracle.trace.jsonl and plots under analysis\drives\<stamp>-cmp\.
# Note: the port's default catch-up matches the oracle race (player 6th); a
# drive recorded with -Leader will differ by design (ledger section 7).
param([Parameter(Mandatory = $true)][string]$Stamp)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$dir = Join-Path $Root "analysis\drives"
$route = Join-Path $dir "$Stamp.route.json"
$bevy = Join-Path $dir "$Stamp.bevy.trace.jsonl"
$oracle = Join-Path $dir "$Stamp.oracle.trace.jsonl"
python (Join-Path $Root "tools\oracle\trace.py") $route $oracle
if ($LASTEXITCODE -ne 0) { throw "oracle replay failed" }
Push-Location (Join-Path $Root "port")
try {
    cargo run -q --release --offline --bin mr-compare -- $oracle $bevy --scale 1 --pos-tol 1169 --deg-tol 2 --window 300 --out (Join-Path $dir "$Stamp-cmp")
} finally { Pop-Location }
