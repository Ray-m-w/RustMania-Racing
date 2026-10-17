# Session 1, step 1: machine inventory (2026-10-11)

Host: Windows 11 Home 10.0.26200, x64.

| Tool | Found | Version | Origin |
|---|---|---|---|
| Git | `C:\Program Files\Git\cmd\git.exe` | git version 2.55.0.windows.5 | pre-existing |
| Git bash | `C:\Program Files\Git\bin\bash.exe` | GNU bash 5.3.15(2)-release (x86_64-pc-cygwin) | ships with Git for Windows |
| Python 3 | `C:\Users\Yev3n\AppData\Local\Programs\Python\Python312\python.exe` | Python 3.12.10 (`py -0p` also lists a uv-managed 3.11.15) | pre-existing |
| VS Build Tools 2022 | `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools` | 17.14.39 (August 2026), installationVersion 17.14.37614.0; MSVC 14.44.35207 | owner-installed |
| Windows SDK | `C:\Program Files (x86)\Windows Kits\10` | 10.0.26100.0 | owner-installed with Build Tools |
| CMake | not on PATH; bundled with Build Tools at `Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe` | 3.31.6-msvc6 | Build Tools "C++ CMake tools" component |
| Ninja | not on PATH; bundled with Build Tools at `Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja\ninja.exe` | 1.12.1 | Build Tools component |
| LLVM/clang | `C:\Program Files\LLVM\bin` (clang, clang-cl) | clang 22.1.8 | pre-existing; not used by this project |
| rustup | `C:\Users\Yev3n\.cargo\bin\rustup.exe` | rustup 1.29.1, rustc 1.98.0 | pre-existing; not needed here |
| winget | present | n/a | not used this session |
| gcc / tcc / ccache | **not present** | n/a | see "Gaps" |

## What psxrecomp needs vs what is here

psxrecomp README "Build requirements" (pinned commit, see BINLIST.toml) for
Windows: Git, Python 3, CMake 3.20+, Ninja, and a C/C++ compiler. It offers two
routes: the bundled `cmake-clang-v1-windows-x64.zip` toolchain ("recommended") or
"Build Tools for Visual Studio 2022 with the Desktop development with C++
workload, plus `winget install Kitware.CMake Ninja-build.Ninja`, and run the
setup from a Developer PowerShell for VS 2022 window." BUILDING.md lists MSVC as
a supported Windows compiler and does not require the bundled pack.

Decision: **Visual Studio Build Tools route, no installs.** Build Tools already
ships CMake 3.31.6 (>= 3.20) and Ninja 1.12.1, which a Developer PowerShell puts
on PATH, so the `winget install Kitware.CMake Ninja-build.Ninja` part is
unnecessary. The bundled toolchain zip was not fetched.

Environment: `tools/devshell.ps1` (this repo) enters the VS x64 developer
environment and puts Git's `bin` ahead of everything else on PATH. That second
part matters: in a plain PowerShell `bash` resolves to
`C:\WINDOWS\system32\bash.exe` (WSL), and psxrecomp's setup script and
`regen_bios.sh` call `bash`.

## Gaps (not blocking session 1)

- **No gcc on PATH.** BUILDING.md: "Developers building overlays locally just
  need `gcc` on `PATH` (the `gcc` tier)"; the `tcc` tier ships only in release
  packages. Without either, runtime-captured overlays stay in the interpreter
  (correct, slower). Not needed to reach the title if the intro and title code
  are in the main EXE; it becomes a session-2 question (MILESTONES: "overlays
  reached during the race are captured and compiled so a second run is
  native"). Any install will be proposed then, not now.
- The bundled toolchain is what psxrecomp's own Windows release CI uses
  (llvm-mingw). An MSVC build is a supported but different path; if the
  framework turns out not to build cleanly under MSVC, the bundled pack is the
  fallback, and its release page is the hash source.
