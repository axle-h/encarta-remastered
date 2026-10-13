import sys, struct
def rd7(b,p):
    n=0;s=0
    while True:
        c=b[p];p+=1;n|=(c&0x7f)<<s;s+=7
        if c<0x80:return n,p
def rstr(b,p):
    l,p=rd7(b,p); return b[p:p+l].decode('utf8'),p+l
b=open(sys.argv[1],'rb').read()
_,p=rstr(b,0); ver=b[p:p+3]; p+=3
enc,p=rstr(b,p)
lastUpd,valid,free,it,maxk=struct.unpack_from('<IBIIH',b,p); p+=15
kf,p=rstr(b,p); root,=struct.unpack_from('<I',b,p); p+=4
print('ver',list(ver),'enc',enc,'lastUpd',lastUpd,'valid',valid,'maxkeys',maxk,'keyfactory',kf,'root',root,'hdr_end',p,file=sys.stderr)
out=open(sys.argv[2],'w')
nodes=0; keys=0
def walk(off,depth):
    global nodes,keys
    nodes+=1
    q=off; n,=struct.unpack_from('<H',b,q); q+=2
    ks=[]
    for i in range(n):
        fo,dl,kl=struct.unpack_from('<IBH',b,q); q+=7
        k=b[q:q+kl].decode('utf8','replace'); q+=kl
        ks.append((fo,dl,k))
    c,=struct.unpack_from('<H',b,q); q+=2
    ch=list(struct.unpack_from('<%di'%c,b,q))
    for i,(fo,dl,k) in enumerate(ks):
        if ch: walk(ch[i],depth+1)
        wid,alt,stop=struct.unpack_from('<IBB',b,fo)
        keys+=1
        out.write(f"{k}\t{fo}\t{dl}\t{wid}\t{alt}\t{stop}\n")
    if ch: walk(ch[-1],depth+1)
walk(root,0)
print('nodes',nodes,'keys',keys,file=sys.stderr)
