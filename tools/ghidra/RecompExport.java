// tools/ghidra/RecompExport.java -- Ghidra postScript driven by tools/ghidra/run.ps1.
//
// Args: <ranges file> <symbols.md> <out dir> [decompile timeout seconds, default 60]
//
// 1. Creates a function at every "F <hex>" entry of the recompiler's
//    generated/SLUS_012.37_full.ranges (disassembling first when needed; when an
//    entry sits inside another function's body, that function is removed and
//    both are recreated so the recompiler's boundary wins).
// 2. Applies PSYQ / loader names from the quarry's docs/spec/symbols.md
//    (rows "| 0xADDR | name | signature | source |"; DEFAULT/FUN_ rows skipped).
// 3. Re-runs analysis on the changes, then writes into <out dir>:
//      decomp/<8-hex-addr>_<name>.c   Ghidra decompiler output, one per function
//      functions.tsv                  addr name size in_recomp callers callees
//      xrefs_data.tsv                 per-instruction data refs (incl. lui/addiu pairs)
//      failures.tsv                   create/decompile failures
// Output is derived from game code: it belongs under analysis/ (gitignored).
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.regex.*;

public class RecompExport extends GhidraScript {
    private PrintWriter fail;

    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        if (a.length < 3) throw new IllegalArgumentException("args: ranges symbols.md outdir [timeout]");
        File ranges = new File(a[0]), syms = new File(a[1]), out = new File(a[2]);
        int timeout = a.length > 3 ? Integer.parseInt(a[3]) : 60;
        File decompDir = new File(out, "decomp");
        decompDir.mkdirs();
        for (File old : Objects.requireNonNull(decompDir.listFiles())) old.delete();
        fail = pw(new File(out, "failures.tsv"));
        fail.println("addr\tstage\tdetail");

        // ---- 0. RAM writable (see MakeRamWritable.java; repeated for -ExportOnly) ----
        runScript("MakeRamWritable.java");

        // ---- 1. recompiler entries ----
        List<Address> entries = new ArrayList<>();
        for (String line : Files.readAllLines(ranges.toPath(), StandardCharsets.UTF_8)) {
            line = line.trim();
            if (line.startsWith("F ")) entries.add(addr(Long.parseLong(line.substring(2).trim(), 16)));
        }
        Set<Address> recomp = new HashSet<>(entries);
        println("RECOMP_ENTRIES " + entries.size());
        FunctionManager fm = currentProgram.getFunctionManager();
        int created = 0, existed = 0, split = 0;
        for (Address e : entries) {
            if (monitor.isCancelled()) break;
            Function f = fm.getFunctionAt(e);
            if (f != null) { existed++; continue; }
            if (getInstructionAt(e) == null) {
                clearDataAt(e);
                new DisassembleCommand(e, null, true).applyTo(currentProgram, monitor);
            }
            Function host = fm.getFunctionContaining(e);
            if (host != null) {
                Address hostEntry = host.getEntryPoint();
                String hostName = host.getName();
                SourceType hostSrc = host.getSymbol().getSource();
                fm.removeFunction(hostEntry);
                boolean ok = create(e);
                Function re = createAt(hostEntry);
                if (re != null && hostSrc != SourceType.DEFAULT) re.setName(hostName, hostSrc);
                if (ok) { split++; created++; }
                else fail.println(e + "\tcreate\tsplit of " + hostEntry + " failed");
                continue;
            }
            if (create(e)) created++;
            else fail.println(e + "\tcreate\tCreateFunctionCmd failed");
        }
        println("FUNCS existed=" + existed + " created=" + created + " (split=" + split + ")");

        // ---- 2. names ----
        Pattern row = Pattern.compile("^\\|\\s*0x([0-9a-fA-F]{8})\\s*\\|\\s*([^|]+?)\\s*\\|[^|]*\\|\\s*(\\w+)\\s*\\|");
        int named = 0;
        Set<Address> seen = new HashSet<>();
        for (String line : Files.readAllLines(syms.toPath(), StandardCharsets.UTF_8)) {
            Matcher m = row.matcher(line);
            if (!m.find()) continue;
            String name = m.group(2), src = m.group(3);
            if (src.equals("DEFAULT") || name.startsWith("FUN_")) continue;
            Address ad = addr(Long.parseLong(m.group(1), 16));
            if (!seen.add(ad)) continue;
            Function f = fm.getFunctionAt(ad);
            if (f == null || f.getName().equals(name)) continue;
            try { f.setName(name, SourceType.USER_DEFINED); named++; }
            catch (Exception ex) { fail.println(ad + "\tname\t" + name + ": " + ex.getMessage()); }
        }
        println("NAMES_APPLIED " + named);

        // ---- 3. re-analyse ----
        analyzeChanges(currentProgram);

        // ---- 4. exports ----
        DecompInterface di = new DecompInterface();
        DecompileOptions opts = new DecompileOptions();
        opts.grabFromProgram(currentProgram);
        di.setOptions(opts);
        di.toggleCCode(true);
        di.toggleSyntaxTree(true);
        di.setSimplificationStyle("decompile");
        if (!di.openProgram(currentProgram)) throw new IllegalStateException("decompiler: " + di.getLastMessage());

        PrintWriter ft = pw(new File(out, "functions.tsv"));
        ft.println("addr\tname\tsize\tin_recomp\tsource\tcaller_count\tcallers\tcallees");
        PrintWriter xt = pw(new File(out, "xrefs_data.tsv"));
        xt.println("func_addr\tfunc_name\tinsn_addr\ttarget\ttarget_name\tref_type\top_index");
        int n = 0, ok = 0, bad = 0, recompDone = 0;
        ReferenceManager rm = currentProgram.getReferenceManager();
        for (Function f : fm.getFunctions(true)) {
            if (monitor.isCancelled()) break;
            n++;
            Address ep = f.getEntryPoint();
            String hex = String.format("%08x", ep.getOffset());
            boolean inR = recomp.contains(ep);
            Set<Function> callers = f.getCallingFunctions(monitor);
            Set<Function> callees = f.getCalledFunctions(monitor);
            ft.println(hex + "\t" + f.getName() + "\t" + f.getBody().getNumAddresses() + "\t" + (inR ? 1 : 0)
                + "\t" + f.getSymbol().getSource() + "\t" + callers.size() + "\t" + list(callers) + "\t" + list(callees));

            InstructionIterator it = currentProgram.getListing().getInstructions(f.getBody(), true);
            while (it.hasNext()) {
                Instruction ins = it.next();
                for (Reference r : ins.getReferencesFrom()) {
                    RefType t = r.getReferenceType();
                    Address to = r.getToAddress();
                    if (!to.isMemoryAddress() || t.isFlow()) continue;
                    Symbol s = getSymbolAt(to);
                    xt.println(hex + "\t" + f.getName() + "\t" + String.format("%08x", ins.getAddress().getOffset())
                        + "\t" + String.format("%08x", to.getOffset()) + "\t" + (s == null ? "" : s.getName())
                        + "\t" + t.getName() + "\t" + r.getOperandIndex());
                }
            }

            String text;
            try {
                DecompileResults res = di.decompileFunction(f, timeout, monitor);
                if (res != null && res.decompileCompleted() && res.getDecompiledFunction() != null) {
                    text = res.getDecompiledFunction().getC();
                    ok++;
                    if (inR) recompDone++;
                } else {
                    String msg = res == null ? "null result" : res.getErrorMessage();
                    msg = msg == null ? "" : msg.replaceAll("\\s+", " ").trim();
                    fail.println(ep + "\tdecompile\t" + msg);
                    text = "/* DECOMPILE FAILED: " + msg + " */\n";
                    bad++;
                }
            } catch (Exception ex) {
                fail.println(ep + "\tdecompile\t" + ex);
                text = "/* DECOMPILE EXCEPTION: " + ex + " */\n";
                bad++;
            }
            String header = "/* " + hex + " " + f.getName() + " size=" + f.getBody().getNumAddresses()
                + (inR ? " recomp" : " ghidra-only") + " */\n";
            try (PrintWriter w = pw(new File(decompDir, hex + "_" + f.getName().replaceAll("[^A-Za-z0-9_.@-]", "_") + ".c"))) {
                w.print(header);
                w.print(text);
            }
            if (n % 200 == 0) println("DECOMP_PROGRESS " + n);
        }
        di.dispose();
        ft.close(); xt.close(); fail.close();
        int missing = 0;
        for (Address e : entries) if (fm.getFunctionAt(e) == null) missing++;
        println("RECOMP_EXPORT_DONE functions=" + n + " decompiled=" + ok + " failed=" + bad
            + " recomp_entries=" + entries.size() + " recomp_decompiled=" + recompDone
            + " recomp_missing_function=" + missing);
    }

    private boolean create(Address e) {
        return createAt(e) != null;
    }

    private Function createAt(Address e) {
        CreateFunctionCmd c = new CreateFunctionCmd(e);
        if (!c.applyTo(currentProgram, monitor)) return null;
        return currentProgram.getFunctionManager().getFunctionAt(e);
    }

    private void clearDataAt(Address e) {
        Data d = getDataContaining(e);
        if (d != null) currentProgram.getListing().clearCodeUnits(d.getMinAddress(), d.getMaxAddress(), false);
    }

    private Address addr(long off) {
        return currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(off);
    }

    private static String list(Set<Function> fs) {
        List<String> s = new ArrayList<>();
        for (Function f : fs) s.add(String.format("%08x", f.getEntryPoint().getOffset()) + ":" + f.getName());
        Collections.sort(s);
        return String.join(",", s);
    }

    private static PrintWriter pw(File f) throws IOException {
        return new PrintWriter(new BufferedWriter(new OutputStreamWriter(new FileOutputStream(f), StandardCharsets.UTF_8)));
    }
}
