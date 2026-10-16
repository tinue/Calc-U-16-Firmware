import sys,struct
from capstone import *
d=open(sys.argv[1],'rb').read()
start=int(sys.argv[2],16); end=int(sys.argv[3],16)
md=Cs(CS_ARCH_ARM,CS_MODE_THUMB)
names={0x40000000:'I2C',0x40008000:'UART',0x4000C000:'CT16B0',0x40010000:'CT16B1',0x40014000:'CT32B0',0x40018000:'CT32B1',0x4001C000:'ADC',0x40038000:'PMU',0x40044000:'IOCON',0x40048000:'SYSCON',0x50000000:'GPIO0',0x50010000:'GPIO1',0x50020000:'GPIO2',0x50030000:'GPIO3',0xE000E000:'SCS'}
def nm(v):
    for b,n in names.items():
        if b<=v<b+(0x4000 if b<0x50000000 else 0x10000): return f'{n}+{v-b:#x}'
    return ''
pc=start
while pc<end:
    insns=list(md.disasm(d[pc:pc+4],pc,1))
    if not insns:
        print(f'{pc:06x}: .short {struct.unpack_from("<H",d,pc)[0]:04x}'); pc+=2; continue
    i=insns[0]; extra=''
    if i.mnemonic=='ldr' and '[pc' in i.op_str:
        try:
            off=int(i.op_str.split('#')[1].rstrip(']'),0)
        except: off=0
        a=((pc+4)&~3)+off
        if a+4<=len(d):
            v=struct.unpack_from('<I',d,a)[0]; extra=f'  ; ={v:#x} {nm(v)}'
    print(f'{pc:06x}: {i.mnemonic:8s} {i.op_str}{extra}')
    pc+=i.size
