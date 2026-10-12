cmake_minimum_required(VERSION 3.20)
set(PSXRECOMP_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/psxrecomp" CACHE PATH
    "Path to psxrecomp")
if(NOT EXISTS "${PSXRECOMP_ROOT}/runtime/runtime.cmake")
    message(FATAL_ERROR
        "PSXRecomp runtime is missing: ${PSXRECOMP_ROOT}/runtime/runtime.cmake\n"
        "This project expects the psxrecomp framework checkout at ${PSXRECOMP_ROOT}.\n"
        "Run: git submodule update --init --recursive\n"
        "If psxrecomp/ is not a submodule, set -DPSXRECOMP_ROOT=<path-to-psxrecomp>.")
endif()

project(MuppetRaceManiaRecomp LANGUAGES C CXX)

# --- Opt-in product surfaces (OFF by default in the framework) -------------
# Scaffold prompts / flags fill these. Without recomp-ui, PSX_RECOMP_UI is OFF.
# The wizard (WIZARD_CMAKE_BLOCK / ENABLE_SETUP_WIZARD) is the first-run disc
# picker; releases are compiled from the committed generated/ C, so it never
# needs to generate or rebuild anything on a player's machine.
# recomp-ui submodule present (PSX_RECOMP_UI defaults ON).
# Full netplay UI -- enable after adding recomp-ui + testing:
# if(EXISTS "${PSXRECOMP_ROOT}/lib/recomp-net/CMakeLists.txt")
#     set(PSX_NETPLAY ON CACHE BOOL "..." FORCE)
# endif()
set(PSX_SETUP_WIZARD ON CACHE BOOL
    "Advertise first-run setup wizard + Generate & rebuild in recomp-ui" FORCE)
include("${PSXRECOMP_ROOT}/runtime/runtime.cmake")

# This project's own mod catalog. The FRAMEWORK stages it -- together with its
# builtin psx.* packages -- into <exe-dir>/mods/bundled, and refuses to
# configure if a catalog exists here but is not declared. Never copy it by
# hand: five titles did, and framework 4cc04be3's rename of the staged
# directory broke all five silently (bead beads-eio.3.101).
set(PSX_PRELOADED_MODS "${CMAKE_CURRENT_SOURCE_DIR}/mods/preloaded")
if(NOT IS_DIRECTORY "${PSX_PRELOADED_MODS}")
    set(PSX_PRELOADED_MODS "NONE")
endif()
# Development packages (mods/dev: the Rust port's oracle bridge) join the
# catalog only in non-Release builds, through a merged copy of both source
# catalogs in the build tree; Release catalogs never list them. Their plugin
# sources (src/mods/mrm_dev_*.c) are likewise linked only outside Release.
if(NOT CMAKE_BUILD_TYPE STREQUAL "Release" AND IS_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/mods/dev")
    set(_MRM_MERGED_MODS "${CMAKE_BINARY_DIR}/mrm-mods-merged")
    file(REMOVE_RECURSE "${_MRM_MERGED_MODS}")
    file(COPY "${PSX_PRELOADED_MODS}/" DESTINATION "${_MRM_MERGED_MODS}")
    file(COPY "${CMAKE_CURRENT_SOURCE_DIR}/mods/dev/packages" DESTINATION "${_MRM_MERGED_MODS}")
    set(PSX_PRELOADED_MODS "${_MRM_MERGED_MODS}")
endif()

psxrecomp_add_game_runtime(psx-runtime
    PRELOADED_MODS_DIR "${PSX_PRELOADED_MODS}"
    # The framework marks its generic host-pacing loader deprecated; this title
    # ships its own Fast Loading package (mrm.enhancement.fast-loading).
    EXCLUDE_BUILTIN_MODS psx.enhancement.fast-loading
    # ENABLE_NETPLAY_IF_PRESENT
    # NETPLAY_LOBBY_URL ws://netplay.retcomm.net:8765
    ENABLE_SETUP_WIZARD
    WINDOW_TITLE "Muppet RaceMania Recompiled"
    DEFAULT_GAME_CONFIG_PATH "game.toml"
    MAX_PLAYERS 2
    # LAUNCHER_BOXART "${CMAKE_CURRENT_SOURCE_DIR}/launcher_assets/img/boxart.tga"
    APP_ICON "${CMAKE_CURRENT_SOURCE_DIR}/assets/psxrecomp.ico"
    # Committed game C (psxrecomp_cli.py generate; git add generated). Release
    # CI configures with -DPSXRECOMP_REQUIRE_GAME_C=ON so a checkout without
    # it fails instead of building a setup host.
    GEN_MARKER "generated/SLUS_012.37_dispatch.c"
    GEN_FULL_GLOB "generated/SLUS_012.37_full_*.c"
    CODEGEN_SETUP_SOURCES "${CMAKE_CURRENT_SOURCE_DIR}/codegen_setup.c"
)

# Trusted mod plugins for this title's own packages (mods/preloaded). A
# .psxmod never carries native code; these are linked in and selected by id
# (psxrecomp README "Trusted game code").
file(GLOB MRM_MOD_PLUGIN_SOURCES CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/src/mods/*.c")
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    list(FILTER MRM_MOD_PLUGIN_SOURCES EXCLUDE REGEX "/mrm_dev_[^/]*\.c$")
endif()
target_sources(psx-runtime PRIVATE ${MRM_MOD_PLUGIN_SOURCES})

# Stage runtime configs next to the exe for direct build/ launches. Only copy
# files that exist — never fail POST_BUILD when game_options.toml is absent.
foreach(_PSX_RUNTIME_CONFIG game.toml game_options.toml)
    if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/${_PSX_RUNTIME_CONFIG}")
        add_custom_command(TARGET psx-runtime POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "${CMAKE_CURRENT_SOURCE_DIR}/${_PSX_RUNTIME_CONFIG}"
                "$<TARGET_FILE_DIR:psx-runtime>/${_PSX_RUNTIME_CONFIG}"
            COMMENT "Staging ${_PSX_RUNTIME_CONFIG}"
            VERBATIM)
    endif()
endforeach()
