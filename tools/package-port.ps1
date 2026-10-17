# Package the Rust + Bevy port (phase 10):
#   dist\mrm-port-<version>-windows-x64.zip
# with exactly three files:
#   muppet-racemania.exe        mr-bevy, release build
#   README-port.txt             port\package\README-port.txt (disc, controls,
#                               data folder, crash log)
#   THIRD-PARTY-LICENSES.txt    every crate compiled into the exe (cargo
#                               metadata's resolve for mr-bevy on Windows x64,
#                               normal dependencies; Cargo.lock pins them):
#                               name, version, license field, repository, and
#                               the crate's own LICENSE / COPYING / NOTICE
#                               files from the local cargo registry, each
#                               distinct text once
# No disc data, no decoded assets, no captures: the zip's file list is
# checked against the three names above before it is written.
# Version: the port's workspace version (port\Cargo.toml [workspace.package]).
#
#   .\tools\package-port.ps1            # build + package
#   .\tools\package-port.ps1 -NoBuild   # package the existing release exe
param([switch]$NoBuild)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Port = Join-Path $Root "port"

$ws = Get-Content (Join-Path $Port "Cargo.toml") -Raw
if ($ws -notmatch '(?ms)\[workspace\.package\].*?^version\s*=\s*"([^"]+)"') { throw "no workspace version" }
$Version = $Matches[1]

Push-Location $Port
try {
    if (-not $NoBuild) {
        # The shipped exe links the MSVC C runtime statically (+crt-static) so
        # it runs on any Windows PC without the VC++ redistributable (owner
        # decision 2026-10-16). Own target dir: dev builds stay dynamic and
        # their cache untouched.
        $prevFlags = $env:RUSTFLAGS
        $env:RUSTFLAGS = "-C target-feature=+crt-static"
        try {
            cargo build --release --offline -p mr-bevy --target-dir target-dist
            if ($LASTEXITCODE -ne 0) { throw "build failed" }
        } finally { $env:RUSTFLAGS = $prevFlags }
    }
    $metaJson = cargo metadata --offline --format-version 1 --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed" }
} finally { Pop-Location }
$meta = ($metaJson -join "`n") | ConvertFrom-Json

# --- the crates linked into mr-bevy.exe (normal dependency edges only)
$pkgs = @{}
foreach ($p in $meta.packages) { $pkgs[$p.id] = $p }
$nodes = @{}
foreach ($n in $meta.resolve.nodes) { $nodes[$n.id] = $n }
$start = ($meta.packages | Where-Object { $_.name -eq "mr-bevy" }).id
$seen = @{}
$queue = New-Object System.Collections.Queue
$queue.Enqueue($start)
while ($queue.Count -gt 0) {
    $id = $queue.Dequeue()
    if ($seen.ContainsKey($id)) { continue }
    $seen[$id] = $true
    foreach ($d in $nodes[$id].deps) {
        $normal = $false
        foreach ($k in $d.dep_kinds) { if ($null -eq $k.kind) { $normal = $true } }
        if ($normal) { $queue.Enqueue($d.pkg) }
    }
}
$third = $seen.Keys | ForEach-Object { $pkgs[$_] } | Where-Object { $null -ne $_.source } | Sort-Object name, version

# --- THIRD-PARTY-LICENSES.txt
$sb = New-Object System.Text.StringBuilder
[void]$sb.AppendLine("Muppet RaceMania (Rust port) ${Version}: third-party software")
[void]$sb.AppendLine("=" * 72)
[void]$sb.AppendLine("")
[void]$sb.AppendLine("muppet-racemania.exe is compiled from the project's own crates (mr-bevy,")
[void]$sb.AppendLine("mr-sim, mr-disc, mr-assets, mr-audio, mr-video, mr-trace) and the")
[void]$sb.AppendLine("$($third.Count) crates below (crates.io; versions and checksums pinned by")
[void]$sb.AppendLine("port/Cargo.lock), plus the Rust standard library (MIT OR Apache-2.0).")
[void]$sb.AppendLine("Each crate's licence files follow the list, each distinct text once.")
[void]$sb.AppendLine("")
$texts = [ordered]@{}
$users = @{}
$noFiles = @()
foreach ($p in $third) {
    $lic = if ($p.license) { $p.license } elseif ($p.license_file) { "see $($p.license_file)" } else { "(none declared)" }
    $repo = if ($p.repository) { "  $($p.repository)" } else { "" }
    [void]$sb.AppendLine(("{0} {1}: {2}{3}" -f $p.name, $p.version, $lic, $repo))
    $dir = Split-Path -Parent $p.manifest_path
    $files = Get-ChildItem -LiteralPath $dir -File | Where-Object { $_.Name -match '^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE|COPYRIGHT)' } | Sort-Object Name
    if ($p.license_file) {
        $lf = Join-Path $dir $p.license_file
        if ((Test-Path -LiteralPath $lf) -and -not ($files | Where-Object { $_.FullName -eq (Resolve-Path -LiteralPath $lf).Path })) { $files = @($files) + @(Get-Item -LiteralPath $lf) }
    }
    if (-not $files) { $noFiles += "$($p.name) $($p.version)" }
    foreach ($f in $files) {
        $t = ([System.IO.File]::ReadAllText($f.FullName)).Replace("`r`n", "`n").Trim()
        if (-not $texts.Contains($t)) { $texts[$t] = $f.Name; $users[$t] = New-Object System.Collections.ArrayList }
        [void]$users[$t].Add("$($p.name) $($p.version)")
    }
}
[void]$sb.AppendLine("")
if ($noFiles.Count -gt 0) {
    [void]$sb.AppendLine("Crates that ship no licence file (their licence field above applies; the")
    [void]$sb.AppendLine("standard texts of MIT and Apache-2.0 appear below with other crates):")
    [void]$sb.AppendLine("  " + ($noFiles -join ", "))
    [void]$sb.AppendLine("")
}
$i = 0
foreach ($t in $texts.Keys) {
    $i++
    [void]$sb.AppendLine("-" * 72)
    [void]$sb.AppendLine("Text $i ($($texts[$t])), used by: $($users[$t] -join ', ')")
    [void]$sb.AppendLine("-" * 72)
    [void]$sb.AppendLine($t)
    [void]$sb.AppendLine("")
}

# --- stage and zip
$Stage = Join-Path $Root "release-stage\port"
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force $Stage | Out-Null
Copy-Item (Join-Path $Port "target-dist\release\mr-bevy.exe") (Join-Path $Stage "muppet-racemania.exe")
$readme = (Get-Content (Join-Path $Port "package\README-port.txt") -Raw).Replace("@VERSION@", $Version).Replace("`r`n", "`n").Replace("`n", "`r`n")
[System.IO.File]::WriteAllText((Join-Path $Stage "README-port.txt"), $readme)
[System.IO.File]::WriteAllText((Join-Path $Stage "THIRD-PARTY-LICENSES.txt"), $sb.ToString().Replace("`r`n", "`n").Replace("`n", "`r`n"))

$want = @("muppet-racemania.exe", "README-port.txt", "THIRD-PARTY-LICENSES.txt") | Sort-Object
$have = Get-ChildItem -Recurse -File $Stage | ForEach-Object { $_.Name } | Sort-Object
if (Compare-Object $want $have) { throw "stage holds unexpected files: $($have -join ', ')" }

$Dist = Join-Path $Root "dist"
New-Item -ItemType Directory -Force $Dist | Out-Null
$Zip = Join-Path $Dist "mrm-port-$Version-windows-x64.zip"
if (Test-Path $Zip) { Remove-Item -Force $Zip }
Compress-Archive -Path (Join-Path $Stage "*") -DestinationPath $Zip
$hash = (Get-FileHash -Algorithm SHA256 $Zip).Hash.ToLower()
"{0}  {1} bytes  sha256 {2}" -f $Zip, (Get-Item $Zip).Length, $hash
"  {0} third-party crates, {1} distinct licence texts" -f $third.Count, $texts.Count
