# Launch the debug-tools (RelWithDebInfo) build for observation over the TCP
# debug server (psxrecomp docs/TCP_COMMANDS.md), with a process guard.
#
#   .\tools\run-dev.ps1 -Start [-Bios <retail dump>] [-Port 4370]
#   python psxrecomp\tools\debug_client.py --port 4370 <cmd> [key=value ...]
#   .\tools\run-dev.ps1 -Stop
#
# -Start refuses to run if a game process from this build is already alive, and
# records the PID in build-dev\run-dev.pid. -Stop sends `quit`, then kills the
# recorded PID if it is still alive, and reports anything left over.
param(
    [switch]$Start,
    [switch]$Stop,
    [switch]$Autocompile,
    [string]$Bios = "",
    [string]$Disc = "",
    [int]$Port = 4370
)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
# RelWithDebInfo links libc++/libunwind dynamically from the toolchain pack
# (only Release links the runtime statically), so the pack's bin/ must be on PATH.
. (Join-Path $PSScriptRoot "devshell.ps1")
$BuildDir = Join-Path $Root "build-dev"
$PidFile = Join-Path $BuildDir "run-dev.pid"

function Get-GameExe {
    $marker = Join-Path $BuildDir "psxrecomp_exe_name-psx-runtime.txt"
    if (Test-Path $marker) {
        $name = (Get-Content $marker -Raw).Trim()
        if (-not $name.EndsWith(".exe")) { $name += ".exe" }
        return (Join-Path $BuildDir $name)
    }
    return (Join-Path $BuildDir "MuppetRaceMania_Recompiled.exe")
}

function Get-Alive {
    $exe = Get-GameExe
    $n = [IO.Path]::GetFileNameWithoutExtension($exe)
    return @(Get-Process -Name $n -ErrorAction SilentlyContinue)
}

if ($Start) {
    $alive = Get-Alive
    if ($alive.Count -gt 0) { throw "already running: PID $($alive.Id -join ', '); run -Stop first" }
    $exe = Get-GameExe
    if (-not (Test-Path $exe)) { throw "missing $exe; build build-dev first" }
    # CMake's POST_BUILD staging only runs when the exe relinks; restage so a
    # config-only edit is never tested against a stale copy.
    foreach ($cfg in "game.toml", "game_options.toml") {
        if (Test-Path (Join-Path $Root $cfg)) { Copy-Item -Force (Join-Path $Root $cfg) (Join-Path $BuildDir $cfg) }
    }
    if (-not $Disc) {
        $Disc = (Select-String -Path (Join-Path $Root "game.toml") -Pattern '^disc\s*=\s*"(.*)"').Matches[0].Groups[1].Value -replace '\\\\', '\'
    }
    if ($Autocompile) {
        # Dev builds stage no overlay_toolchain/ beside the exe, so the runtime
        # leaves capture-and-compile off. Give it the same command it would build
        # from a bundle (runtime/src/main.cpp build_toolchain_cmd), pointed at this
        # checkout's emitter and the pack's gcc. Flags per docs/COMPILING_OVERLAYS.md.
        $q = { param($s) '"' + $s + '"' }
        $env:PSX_OVERLAY_AUTOCOMPILE_CMD = (@(
            (& $q $env:RETCOMM_PYTHON), (& $q (Join-Path $Root "psxrecomp\tools\compile_overlays.py")),
            "--captures", (& $q (Join-Path $BuildDir "overlay_captures.json")),
            "--game-toml", (& $q (Join-Path $BuildDir "game.toml")),
            "--recompiler", (& $q (Join-Path $Root "build-recompiler\psxrecomp-game.exe")),
            "--runtime-include", (& $q (Join-Path $Root "psxrecomp\runtime\include")),
            "--project-root", (& $q (Join-Path $Root "psxrecomp")),
            "--out-dir", (& $q (Join-Path $BuildDir "cache")),
            "--cps", "--compiler", "gcc", "--gcc", (& $q (Join-Path $env:PSXRECOMP_TOOLCHAIN_DIR "bin\gcc.exe"))
        ) -join " ")
        $env:PSX_OVERLAY_AUTOCOMPILE_CWD = $Root
    }
    $argv = @("--no-launcher", "--game", "`"$(Join-Path $BuildDir 'game.toml')`"", "--disc", "`"$Disc`"", "--debug-port", "$Port")
    if ($Bios) { $argv += @("--bios", "`"$Bios`"") }
    $p = Start-Process -FilePath $exe -ArgumentList $argv -WorkingDirectory $BuildDir -PassThru
    Set-Content -Path $PidFile -Value $p.Id
    "started PID $($p.Id): $exe $($argv -join ' ')"
}

if ($Stop) {
    $py = Join-Path $Root "psxrecomp\tools\debug_client.py"
    try { python $py --port $Port quit 2>$null | Out-Null } catch {}
    Start-Sleep -Seconds 2
    if (Test-Path $PidFile) {
        $id = [int](Get-Content $PidFile)
        $p = Get-Process -Id $id -ErrorAction SilentlyContinue
        if ($p) { Stop-Process -Id $id -Force; "killed PID $id (did not exit on quit)" } else { "PID $id exited" }
        Remove-Item $PidFile
    }
    $left = Get-Alive
    if ($left.Count -gt 0) { "LEFTOVER: PID $($left.Id -join ', ')" } else { "no game processes left" }
}
