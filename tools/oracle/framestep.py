"""Exploration aid: run the whole race frame FUN_8007916C under mipsref
(GTE lenient, rand/MulMatrix0 stubbed) and report writes to kart 0.
    python tools/oracle/framestep.py VA VB  |  python tools/oracle/framestep.py calls VA VB
"""
import sys,struct
sys.path.insert(0,'tools/oracle')
from refstep import *
from mipsref import s16 as S16
def mulmat(mm):
    a,b,c=mm.r[4],mm.r[5],mm.r[6]
    A=[[S16(mm.r16(a+2*(3*i+j))) for j in range(3)] for i in range(3)]
    B=[[S16(mm.r16(b+2*(3*i+j))) for j in range(3)] for i in range(3)]
    for i in range(3):
        for j in range(3):
            v=sum(A[i][k]*B[k][j] for k in range(3))>>12
            mm.w16(c+2*(3*i+j),max(-32768,min(32767,v)))
    return c
def run_frame(c,va,vb,watch=(0,0x2f4,0x2fc)):
    m=c.machine(va)
    m.w16(0x800B0CE8,struct.unpack('<H',c.read(vb,0x800B0CE8,2))[0])
    m.w32(0x800B0AEC,struct.unpack('<I',c.read(vb,0x800B0AEC,4))[0])
    m.stubs[0x80093350]=lambda mm: 0x1234
    m.stubs[0x800940b0]=mulmat
    for _b in (0xA0, 0xB0, 0xC0, 0x800259DC): m.stubs[_b]=lambda mm: 0
    m.gte_lenient=True
    writers=[]
    orig=m.w32
    calls=[]
    m.on_call=lambda t,mm: calls.append(t)
    def w32(a,v):
        if (a&0x1fffff) in [((KART0+o)&0x1fffff) for o in watch]: writers.append((hex(m.cur),hex(calls[-1]) if calls else '',v-(1<<32) if v&0x80000000 else v))
        orig(a,v)
    m.w32=w32
    try:
        m.call(0x8007916C)
    except Exception as e: print('ERR',e,hex(m.cur))
    return m,writers
if __name__=='__main__' and sys.argv[1] not in ('calls','post','pre'):
    c=Cap('analysis/oracle/lap1P3')
    va,vb=int(sys.argv[1]),int(sys.argv[2])
    m,w=run_frame(c,va,vb)
    o=KART0&0x1fffff
    a=struct.unpack_from('<iii',m.ram,o); r=struct.unpack_from('<iii',c.read(vb,KART0,12))
    print('pos diff',[x-y for x,y in zip(a,r)])
    a=struct.unpack_from('<iii',m.ram,o+0x2f4); r=struct.unpack_from('<iii',c.read(vb,KART0+0x2f4,12))
    print('vel diff',[x-y for x,y in zip(a,r)])
    for x in w: print(x)

def last_calls(c,va,vb):
    m=c.machine(va)
    m.stubs[0x80093350]=lambda mm: 0x1234
    m.stubs[0x800940b0]=mulmat
    for _b in (0xA0, 0xB0, 0xC0, 0x800259DC): m.stubs[_b]=lambda mm: 0
    m.gte_lenient=True
    calls=[]
    m.on_call=lambda t,mm: calls.append((hex(t),hex(mm.cur)))
    try: m.call(0x8007916C)
    except Exception as e: print('ERR',e)
    print(calls[-8:])

if __name__=='__main__' and sys.argv[1]=='calls':
    last_calls(Cap('analysis/oracle/lap1P3'),int(sys.argv[2]),int(sys.argv[3]))

def post(c, va, vb):
    """Kart 0 update, then each race-frame callee in turn; report which one
    changes kart 0's position/velocity."""
    m=c.machine(va)
    m.w16(0x800B0CE8,struct.unpack('<H',c.read(vb,0x800B0CE8,2))[0])
    m.w32(0x800B0AEC,struct.unpack('<I',c.read(vb,0x800B0AEC,4))[0])
    m.stubs[0x80093350]=lambda mm: 0x1234
    m.stubs[0x800940b0]=mulmat
    for _b in (0xA0, 0xB0, 0xC0, 0x800259DC, 0x80025878): m.stubs[_b]=lambda mm: 0
    m.gte_lenient=True
    o=KART0&0x1fffff
    snap=lambda: struct.unpack_from('<iiiiii',m.ram,o)[:3]+struct.unpack_from('<iii',m.ram,o+0x2f4)
    kart_update(m)
    print('after kart0 update', snap())
    n=m.r16(0x800B0DFE)+m.r16(0x800B0C98)
    seq=[('AI karts',None)]+[(hex(f),f) for f in (0x80086DF0,0x800837F8,0x80025D8C,0x800617CC,0x8006C388,0x8007F03C,0x8007A9DC,0x8007BBF0,0x80069648,0x8007D304,0x8008650C,0x8008727C,0x800327D0,0x80033B98,0x80035004,0x80078F38)]
    for name,f in seq:
        before=snap()
        try:
            if f is None:
                for k in range(1,6):
                    kk=KART0+0x504*k
                    b=m.call(0x800912F8,kk) if (m.r16(kk+0x496)&0x60)==0 else 0
                    m.call(0x80064B5C,kk,b&0xffff if not b&0x8000 else b|0xffff0000,0xffffffff)
            elif f in (0x80086DF0,0x80025D8C):
                m.call(f,KART0,n)
            else:
                m.call(f, limit=3_000_000)
        except Exception as e:
            print(name,'ERR',e)
        after=snap()
        if after!=before: print(name,'changed',[a-b for a,b in zip(after,before)])
    r=struct.unpack_from('<iii',c.read(vb,KART0,12))+struct.unpack_from('<iii',c.read(vb,KART0+0x2f4,12))
    print('oracle - ours', [a-b for a,b in zip(r,snap())])

if __name__=='__main__' and sys.argv[1]=='post':
    post(Cap('analysis/oracle/lap1P3'),int(sys.argv[2]),int(sys.argv[3]))

def pre(c, va, vb, funcs=(0x800306D0, 0x8003C5FC, 0x80063A98, 0x800418D8, 0x80041A20, 0x80041CC4)):
    """Race-frame prologue (before the kart loop), then kart 0's update;
    compare kart 0 with the oracle at vb."""
    m=c.machine(va)
    m.w16(0x800B0CE8,struct.unpack('<H',c.read(vb,0x800B0CE8,2))[0])
    m.w32(0x800B0AEC,struct.unpack('<I',c.read(vb,0x800B0AEC,4))[0])
    m.stubs[0x80093350]=lambda mm: 0x1234
    m.stubs[0x800940b0]=mulmat
    for _b in (0xA0, 0xB0, 0xC0, 0x800259DC, 0x80025878): m.stubs[_b]=lambda mm: 0
    m.gte_lenient=True
    for f in funcs:
        try: m.call(f, limit=3_000_000)
        except Exception as e: print(hex(f),'ERR',e)
    kart_update(m)
    o=KART0&0x1fffff
    ours=bytes(m.ram[o:o+KSZ]); ref=c.read(vb,KART0,KSZ)
    d=[(hex(off),struct.unpack_from('<h',ours,off)[0],struct.unpack_from('<h',ref,off)[0]) for off in range(0,KSZ,2) if ours[off:off+2]!=ref[off:off+2]]
    print('rng', hex(m.r32(0x800B0BCC)), hex(struct.unpack('<I',c.read(vb,0x800B0BCC,4))[0]))
    return d

if __name__=='__main__' and sys.argv[1]=='pre':
    d=pre(Cap('analysis/oracle/lap1P3'),int(sys.argv[2]),int(sys.argv[3]))
    for x in d: print(x)
