// tools/ghidra/MakeRamWritable.java -- preScript (and called again by RecompExport).
// ghidra_psx_ldr maps the EXE as CODE 80010000-800b0fff r-x and the rest of RAM
// r-x. The decompiler treats non-writable memory as constant, so game globals in
// .data/.bss get folded to their load-time values and live branches are removed
// as "unreachable". Mark every RAM-range block (8000xxxx-801fffff) writable.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.mem.MemoryBlock;

public class MakeRamWritable extends GhidraScript {
    @Override
    public void run() throws Exception {
        int n = 0;
        for (MemoryBlock b : currentProgram.getMemory().getBlocks()) {
            long lo = b.getStart().getOffset();
            if (lo >= 0x80000000L && lo < 0x80200000L && !b.isWrite()) {
                b.setWrite(true);
                n++;
            }
        }
        println("RAM_BLOCKS_MADE_WRITABLE " + n);
    }
}
