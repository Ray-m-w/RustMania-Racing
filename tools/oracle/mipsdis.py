"""Tiny MIPS R3000 disassembler for reading the game's code (reading aid).

    python tools/oracle/mipsdis.py RAM.bin START END [--cov cov.txt]
"""
import struct
import sys

R = ["zero", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5",
     "t6", "t7", "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "t8", "t9", "k0", "k1",
     "gp", "sp", "fp", "ra"]
GP = 0x800AFEC8


def s16(v):
    v &= 0xFFFF
    return v - 0x10000 if v & 0x8000 else v


def dis(ins, pc):
    op = ins >> 26
    rs, rt, rd = (ins >> 21) & 31, (ins >> 16) & 31, (ins >> 11) & 31
    sa, fn, imm = (ins >> 6) & 31, ins & 63, s16(ins)
    if ins == 0:
        return "nop"
    if op == 0:
        names = {0x20: "add", 0x21: "addu", 0x22: "sub", 0x23: "subu", 0x24: "and", 0x25: "or",
                 0x26: "xor", 0x27: "nor", 0x2A: "slt", 0x2B: "sltu"}
        if fn in names:
            if fn == 0x21 and rt == 0:
                return "move %s, %s" % (R[rd], R[rs])
            return "%s %s, %s, %s" % (names[fn], R[rd], R[rs], R[rt])
        sh = {0: "sll", 2: "srl", 3: "sra"}
        if fn in sh:
            return "%s %s, %s, %d" % (sh[fn], R[rd], R[rt], sa)
        shv = {4: "sllv", 6: "srlv", 7: "srav"}
        if fn in shv:
            return "%s %s, %s, %s" % (shv[fn], R[rd], R[rt], R[rs])
        if fn == 8:
            return "jr %s" % R[rs]
        if fn == 9:
            return "jalr %s, %s" % (R[rd], R[rs])
        md = {0x18: "mult", 0x19: "multu", 0x1A: "div", 0x1B: "divu"}
        if fn in md:
            return "%s %s, %s" % (md[fn], R[rs], R[rt])
        if fn == 0x10:
            return "mfhi %s" % R[rd]
        if fn == 0x12:
            return "mflo %s" % R[rd]
        if fn == 0x11:
            return "mthi %s" % R[rs]
        if fn == 0x13:
            return "mtlo %s" % R[rs]
        if fn == 0x0C:
            return "syscall"
        if fn == 0x0D:
            return "break 0x%x" % ((ins >> 6) & 0xFFFFF)
        return "special?%02x" % fn
    br = (pc + 4 + (imm << 2)) & 0xFFFFFFFF
    if op == 1:
        nm = {0: "bltz", 1: "bgez", 16: "bltzal", 17: "bgezal"}.get(rt, "regimm?")
        return "%s %s, %08X" % (nm, R[rs], br)
    if op in (2, 3):
        t = (pc & 0xF0000000) | ((ins & 0x3FFFFFF) << 2)
        return "%s %08X" % ("j" if op == 2 else "jal", t)
    if op == 4:
        if rs == 0 and rt == 0:
            return "b %08X" % br
        return "beq %s, %s, %08X" % (R[rs], R[rt], br)
    if op == 5:
        return "bne %s, %s, %08X" % (R[rs], R[rt], br)
    if op == 6:
        return "blez %s, %08X" % (R[rs], br)
    if op == 7:
        return "bgtz %s, %08X" % (R[rs], br)
    imms = {8: "addi", 9: "addiu", 10: "slti", 11: "sltiu"}
    if op in imms:
        if op == 9 and rs == 0:
            return "li %s, %d" % (R[rt], imm)
        return "%s %s, %s, %d" % (imms[op], R[rt], R[rs], imm)
    if op == 12:
        return "andi %s, %s, 0x%x" % (R[rt], R[rs], ins & 0xFFFF)
    if op == 13:
        return "ori %s, %s, 0x%x" % (R[rt], R[rs], ins & 0xFFFF)
    if op == 14:
        return "xori %s, %s, 0x%x" % (R[rt], R[rs], ins & 0xFFFF)
    if op == 15:
        return "lui %s, 0x%x" % (R[rt], ins & 0xFFFF)
    mem = {0x20: "lb", 0x21: "lh", 0x22: "lwl", 0x23: "lw", 0x24: "lbu", 0x25: "lhu", 0x26: "lwr",
           0x28: "sb", 0x29: "sh", 0x2A: "swl", 0x2B: "sw", 0x2E: "swr", 0x32: "lwc2", 0x3A: "swc2"}
    if op in mem:
        extra = ""
        if rs == 28:
            extra = "   ; [%08X]" % ((GP + imm) & 0xFFFFFFFF)
        reg = R[rt] if op not in (0x32, 0x3A) else "gte%d" % rt
        if imm < 0:
            return "%s %s, -0x%x(%s)%s" % (mem[op], reg, -imm, R[rs], extra)
        return "%s %s, 0x%x(%s)%s" % (mem[op], reg, imm, R[rs], extra)
    if op == 0x12:
        if ins & (1 << 25):
            return "cop2 0x%07x" % (ins & 0x1FFFFFF)
        return {0: "mfc2", 2: "cfc2", 4: "mtc2", 6: "ctc2"}.get(rs, "cop2?") + " %s, $%d" % (R[rt], rd)
    if op == 0x10:
        return "cop0 %08x" % ins
    return ".word 0x%08x" % ins


def main():
    ram = open(sys.argv[1], "rb").read()
    a0 = int(sys.argv[2], 16)
    a1 = int(sys.argv[3], 16)
    cov = None
    if "--cov" in sys.argv:
        cov = set(int(x, 16) for x in open(sys.argv[sys.argv.index("--cov") + 1]).read().split())
    lui = {}
    for pc in range(a0, a1, 4):
        ins = struct.unpack_from("<I", ram, pc & 0x1FFFFF)[0]
        mark = ""
        if cov is not None:
            mark = "* " if pc in cov else "  "
        d = dis(ins, pc)
        # resolve lui/addiu|lw pairs (best effort)
        op = ins >> 26
        if op == 15:
            lui[(ins >> 16) & 31] = (ins & 0xFFFF) << 16
        elif op in (9, 0x20, 0x21, 0x23, 0x24, 0x25, 0x28, 0x29, 0x2B) and ((ins >> 21) & 31) in lui:
            d += "   ; =%08X" % ((lui[(ins >> 21) & 31] + s16(ins)) & 0xFFFFFFFF)
        print("%s%08X: %s" % (mark, pc, d))


if __name__ == "__main__":
    main()
