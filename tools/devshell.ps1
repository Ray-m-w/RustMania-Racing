# Enter the build environment used by this project, in the current PowerShell.
#
#   . .\tools\devshell.ps1
#
# Toolchain: psxrecomp's bundled cmake-clang-v1 pack (llvm-mingw clang, cmake,
# ninja, SDL3/zlib under deps/), unpacked outside the repo. Override the
# location with $env:MRR_TOOLCHAIN_DIR. This mirrors the pack's own env.bat and
# psxrecomp README "Bundled toolchain" lines; nothing is installed or persisted.
#
# The owner's VS Build Tools (MSVC) cannot build the pinned framework (see
# docs/notes/SESSION-1-tools.md), so MSVC is deliberately not on PATH here.
#
# Git for Windows' bin goes first so `bash` is Git bash, not WSL's
# C:\Windows\System32\bash.exe (psxrecomp's scripts call `bash`).
$ErrorActionPreference = "Stop"
$pack = if ($env:MRR_TOOLCHAIN_DIR) { $env:MRR_TOOLCHAIN_DIR } else { "G:\GameDev\retcomm-toolchain\pack" }
if (-not (Test-Path (Join-Path $pack "bin\clang.exe"))) { throw "toolchain pack not found at $pack" }

$env:PSXRECOMP_TOOLCHAIN_DIR = $pack
$env:RETCOMM_TOOLCHAIN_DIR = $pack
$env:RETCOMM_PYTHON = Join-Path $pack "python\python.exe"
$env:CC = Join-Path $pack "bin\clang.exe"
$env:CXX = Join-Path $pack "bin\clang++.exe"
$env:AR = Join-Path $pack "bin\llvm-ar.exe"
$env:RANLIB = Join-Path $pack "bin\llvm-ranlib.exe"
Remove-Item Env:PYTHONHOME -ErrorAction SilentlyContinue
Remove-Item Env:PYTHONPATH -ErrorAction SilentlyContinue
$env:PYTHONNOUSERSITE = "1"
if (Test-Path (Join-Path $pack "deps\include\zlib.h")) { $env:ZLIB_ROOT = Join-Path $pack "deps" }
if (Test-Path (Join-Path $pack "deps\lib\cmake\SDL3")) { $env:SDL3_DIR = Join-Path $pack "deps\lib\cmake\SDL3" }
$env:Path = "C:\Program Files\Git\bin;" + (Join-Path $pack "bin") + ";" + (Join-Path $pack "python") + ";" + $env:Path
