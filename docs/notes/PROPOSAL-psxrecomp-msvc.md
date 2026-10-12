# Proposal (not applied): psxrecomp MSVC build fixes

Status: PROPOSED to the owner, 2026-10-11. Not applied: CLAUDE.md forbids
modifying the framework. This project builds with psxrecomp's bundled
cmake-clang-v1 toolchain instead, so nothing here blocks it.

Framework pin: RetroPortingToolKit/psxrecomp @ 2a4824fcb897cb635438958f0c8cffb5a27df656.
Compiler: MSVC 19.44.35228 (VS Build Tools 17.14.39), CMake 3.31.6-msvc6, Ninja 1.12.1.

## Docs vs behavior

README "Build requirements" and BUILDING.md list "Build Tools for Visual Studio
2022" / MSVC as a supported Windows toolchain. At this pin the runtime does not
compile under MSVC. psxrecomp's own Windows CI uses llvm-mingw / MSYS2, so the
MSVC path is not exercised.

## Failures, verbatim

1. `cmake --build recompiler/build` (BUILDING.md step 1), target `guest_tty_test`:
   `vcruntime_c11_stdatomic.h(16): fatal error C1189: #error: "C atomics require C11 or later"`
   for `runtime/src/guest_tty.c`.
2. `cmake --build runtime/build --target psx-runtime` (step 3):
   - `guest_tty.c`, `render_thread.c`, `gpu_gl_renderer.c`:
     `vcruntime_c11_stdatomic.h(12): fatal error C1189: #error: "C atomic support is not enabled"`
   - `runtime/src/main.cpp(18460): error C2131: expression did not evaluate to a constant`
     (the `static_assert(!frame_interpolation_offered && !skip_fmv_offered, ...)`).

## Cause

- `runtime/runtime.cmake:2511-2517`: under `elseif(MSVC)` the
  `/experimental:c11atomics` switch is applied to exactly one source,
  `runtime/src/audio_trace.c`. Three more C sources now include `<stdatomic.h>`.
  `recompiler/CMakeLists.txt:396` builds `guest_tty_test` from
  `runtime/src/guest_tty.c` with no C standard or atomics option at all.
- `main.cpp:18460` asserts on values MSVC does not accept as constant
  expressions in that context; GCC/Clang do.

## Proposed fix (for upstream, as a focused PR)

- Apply `/std:c17 /experimental:c11atomics` to every runtime C source that
  includes `<stdatomic.h>` (or to all C sources of the target via
  `$<$<COMPILE_LANGUAGE:C>:...>` under the Ninja generator, keeping the per-source
  form for Visual Studio generators as the existing comment requires), and the
  same for `guest_tty_test` in `recompiler/CMakeLists.txt`.
- Make the `main.cpp` `static_assert` operands `constexpr` (or demote it to a
  runtime assert if the values are not compile-time constants under every
  compiler).
- Either add an MSVC job to CI or drop MSVC from the README's supported list.

## Also observed (test drift, not MSVC-specific)

`ctest` at this pin, clang toolchain: 124/132 pass. Failing, none in codegen:
`bundled_release_layout` (test expects `BIOS backends linked: .*SCPH1001` in the
CI template, which now says SCPH-1001 is local-only since #575);
`gl_internal_scale_guards`, `native_wide_line_batching`,
`mod_owned_display_controls` (source-text guards that look for
`static void flush_flat_batch(void)`, now a macro over `flush_flat_batch_impl`);
`optional_netplay_auth` (`recomp_net/host_ice.h` not found);
`aot_overlay_discovery` (asserts a fixed MSYS2 MinGW gcc path);
`ws_cull_edge_interp_test`, `draw_distance_interp_test` (do not link with lld:
undefined symbols that the test's comment says link-time dead-code removal should
drop). BUILDING.md's "38 tests" is also out of date (132 registered).
