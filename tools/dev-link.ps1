# Link the development data into the repo (or unlink it).
#
#   .\tools\dev-link.ps1            # create the links
#   .\tools\dev-link.ps1 -Unlink    # remove the links (the data stays put)
#   .\tools\dev-link.ps1 -DevDir D:\somewhere\RustMania Racing Development
#
# The repo holds only source. Everything needed to keep verifying the port
# against the original lives in the development folder (default
# G:\GameDev\RustMania Racing Development, or $env:MRM_DEV):
#   analysis\   oracle captures and recordings the tests / checkers read
#   build-dev\  the psxrecomp debug build of the original (the oracle)
#   generated\  its generated C (from your disc; never committed)
#   disc\       the PS-X executable extracted from your disc
#   saves\      oracle savestates (slots 1 and 2 are the reference states)
#   ghidra\     the Ghidra project
#   quarry\     Ghidra install, JDK, symbol lists (used by tools\ghidra\run.ps1)
# This script makes directory junctions with the same names in the repo root,
# so tools\run-dev.ps1, tools\oracle\* and the tests find them where they
# expect. All of these names are gitignored.
param([switch]$Unlink, [string]$DevDir = "")
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not $DevDir) { $DevDir = if ($env:MRM_DEV) { $env:MRM_DEV } else { "G:\GameDev\RustMania Racing Development" } }
$names = "analysis", "build-dev", "generated", "disc", "saves", "ghidra"

foreach ($n in $names) {
    $link = Join-Path $Root $n
    $item = Get-Item -LiteralPath $link -Force -ErrorAction SilentlyContinue
    if ($Unlink) {
        if ($item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            # rmdir on a junction removes only the link, never the target's files
            cmd /c rmdir "$link" | Out-Null
            "unlinked $n"
        } elseif ($item) {
            "skipped $n (a real folder, not a link)"
        }
        continue
    }
    $target = Join-Path $DevDir $n
    if (-not (Test-Path -LiteralPath $target)) { "missing $target"; continue }
    if ($item) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { "linked already: $n"; continue }
        "skipped $n (a real folder exists in the repo)"; continue
    }
    New-Item -ItemType Junction -Path $link -Target $target | Out-Null
    "linked $n -> $target"
}
