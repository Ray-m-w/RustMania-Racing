# tools/ghidra/run.ps1 -- headless Ghidra analysis of SLUS_012.37 with the
# recompiler's function list applied, plus a decompiled-C export.
#
#   .\tools\ghidra\run.ps1               import + full analysis + export (fresh project)
#   .\tools\ghidra\run.ps1 -ExportOnly   reuse the existing project, rerun the export only
#
# Ghidra, the JDK and the ghidra_psx_ldr extension are used in place from the
# quarry (F:\Muppet Racemania, read-only; nothing is written there). JAVA_HOME
# is set for this process only. Loader/processor ids are the quarry's
# tools/ghidra/run_analysis.bat (ghidra_psx_ldr@3050da64 PsxLoader.java).
# Outputs: ghidra/projects/muppet.gpr, analysis/ghidra/* (both gitignored).
param(
    [switch]$ExportOnly,
    [int]$DecompileTimeout = 60,
    [string]$Quarry = 'F:\Muppet Racemania'
)
$ErrorActionPreference = 'Stop'
$repo    = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$ghidra  = Join-Path $Quarry '.cache\ghidra\ghidra_12.1.4_PUBLIC'
$jdk     = Join-Path $Quarry 'third_party\jdk-21.0.12.1+1\jdk-21.0.12.1+1'
$symbols = Join-Path $Quarry 'docs\spec\symbols.md'
$exe     = Join-Path $repo 'disc\SLUS_012.37'
$ranges  = Join-Path $repo 'generated\SLUS_012.37_full.ranges'
$projDir = Join-Path $repo 'ghidra\projects'
$outDir  = Join-Path $repo 'analysis\ghidra'
$log     = Join-Path $outDir 'ghidra-run.log'

foreach ($p in $ghidra, $jdk, $symbols, $exe, $ranges) {
    if (-not (Test-Path $p)) { throw "missing input: $p" }
}
$want = '81c6846ef44ba06063bb34197a590439b848ad330270c84ff5f6fc201ba870f8'
$got  = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
if ($got -ne $want) { throw "SLUS_012.37 sha256 $got != $want (quarry BINLIST)" }

New-Item -ItemType Directory -Force $projDir, $outDir | Out-Null
$env:JAVA_HOME = $jdk   # this process (and children) only

$hl = Join-Path $ghidra 'support\analyzeHeadless.bat'
$common = @('-preScript', 'MakeRamWritable.java',
            '-postScript', 'RecompExport.java', $ranges, $symbols, $outDir, "$DecompileTimeout",
            '-scriptPath', $PSScriptRoot)
if ($ExportOnly) {
    $hlArgs = @($projDir, 'muppet', '-process', 'SLUS_012.37', '-noanalysis') + $common
} else {
    $hlArgs = @($projDir, 'muppet', '-overwrite', '-import', $exe,
                '-loader', 'PsxLoader', '-processor', 'PSX:LE:32:default') + $common +
              @('-analysisTimeoutPerFile', '5400')
}
Write-Host "analyzeHeadless -> $log"
$argLine = ($hlArgs | ForEach-Object { '"' + $_ + '"' }) -join ' '
$p = Start-Process -FilePath $hl -ArgumentList $argLine -NoNewWindow -Wait -PassThru `
        -RedirectStandardOutput $log -RedirectStandardError ($log -replace '\.log$', '.err.log')
$rc = $p.ExitCode
Write-Host "HEADLESS_EXIT=$rc"
Select-String -Path $log -Pattern 'RECOMP_ENTRIES|FUNCS existed|NAMES_APPLIED|RECOMP_EXPORT_DONE|ERROR' |
    ForEach-Object { $_.Line }
if ($rc -ne 0) { throw "analyzeHeadless exit $rc (see $log)" }
