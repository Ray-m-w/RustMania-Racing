# tools/ghidra

Headless Ghidra project of `disc/SLUS_012.37` with the recompiler's function
list applied, and Ghidra's decompiler output for every function, to read next
to `generated/SLUS_012.37_full_*.c`.

    .\tools\ghidra\run.ps1               # import + analysis + export (fresh project)
    .\tools\ghidra\run.ps1 -ExportOnly   # reuse the project, redo the export only

Inputs: `disc/SLUS_012.37` (sha256 checked), `generated/SLUS_012.37_full.ranges`
(`F <entry>` lines), names from the quarry's `docs/spec/symbols.md`.
Ghidra 12.1.4, Temurin JDK 21 and ghidra_psx_ldr are used in place from
`F:\Muppet Racemania` (read-only; see BINLIST.toml). `JAVA_HOME` is set for the
run only.

Outputs (all gitignored; they are derived from game code, never commit them):

- `ghidra/projects/muppet.gpr` -- the Ghidra project (open it in the GUI).
- `analysis/ghidra/decomp/<addr>_<name>.c` -- one file per function.
- `analysis/ghidra/functions.tsv` -- addr, name, size, in_recomp, source,
  caller count, callers, callees.
- `analysis/ghidra/xrefs_data.tsv` -- every non-flow memory reference per
  instruction (READ / WRITE / DATA / PARAM), including lui+addiu constants
  Ghidra resolved. `grep 800a1234` answers "who touches this global".
- `analysis/ghidra/failures.tsv`, `analysis/ghidra/ghidra-run.log`.

Scripts: `MakeRamWritable.java` (preScript; the PSX loader maps RAM/CODE
r-x, which makes the decompiler fold globals to their load-time values and
drop live branches as "unreachable" -- marking RAM writable removes that),
`RecompExport.java` (postScript: creates every recompiler entry, splitting a
Ghidra function when an entry falls inside it; applies symbols.md names;
writes the exports). Ghidra decompiles by flow, so a recompiler entry whose
code falls through into the next entry is shown with that continuation.

Expected (2026-10-12): 1443 recompiler entries, all present and decompiled;
1909 functions total (the rest: 153 GTE macro stubs at 0x2000xxxx, PSYQ
sub-object labels, 5 stubs in BSS).

Pitfalls (from the quarry's tools/ghidra/README.md): Ghidra compiles every
`.java` in `-scriptPath`, so keep only working scripts in this directory;
Ghidra 12.1.4 needs Java 21; the extension lives only in
`%APPDATA%\ghidra\ghidra_12.1.4_PUBLIC\Extensions`.
