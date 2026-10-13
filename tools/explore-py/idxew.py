import sys,struct
f=open(sys.argv[1],'rb'); off=int(sys.argv[2])
idxe={}
for line in open(sys.argv[3]):
    x=line.split('\t'); idxe[int(x[0])]=(x[2],x[6])
f.seek(off); b=f.read(2_000_000); p=0
def u24(p): return b[p]|b[p+1]<<8|b[p+2]<<16
n=b[p];p+=1
print('altIds',n)
for a in range(n):
    alt=b[p];p+=1
    c1,=struct.unpack_from('<H',b,p);p+=2
    t1=[]
    for i in range(c1):
        ie=u24(p); wo=b[p+3]; rank,ref=struct.unpack_from('<HI',b,p+4); p+=10; t1.append((ie,wo,rank,ref))
    c2,=struct.unpack_from('<H',b,p);p+=2
    t2=[]
    for i in range(c2):
        ie=u24(p); rank,ref,para=struct.unpack_from('<HII',b,p+3); p+=13; t2.append((ie,rank,ref&0x7fffffff,hex(para)))
    c3,=struct.unpack_from('<H',b,p);p+=2
    t3=[]
    for i in range(c3):
        ie=u24(p); wo=b[p+3]; ref,para=struct.unpack_from('<IH',b,p+4); p+=10; t3.append((ie,wo,ref&0x7fffffff,para))
    print(f'alt {alt}: T1(title) {c1}  T2 {c2}  T3(fulltext) {c3}')
    for t in t1[:4]: print('  T1',t, idxe.get(t[0]))
    for t in t2[:3]: print('  T2',t, idxe.get(t[0]))
    for t in t3[:3]: print('  T3',t, idxe.get(t[0]))
print('size',p)
