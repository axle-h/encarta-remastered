import sys,struct,collections
def rd7(b,p):
    n=0;s=0
    while True:
        c=b[p];p+=1;n|=(c&0x7f)<<s;s+=7
        if c<0x80:return n,p
def rstr(b,p):
    l,p=rd7(b,p); return b[p:p+l].decode('utf8'),p+l
b=open(sys.argv[1],'rb').read(); p=0
out=open(sys.argv[2],'w')
for coll in range(2):
    if p>=len(b): break
    n,lcid=struct.unpack_from('<ii',b,p); p+=8
    cls=collections.Counter(); pre=collections.Counter()
    for i in range(n):
        ref,cid=struct.unpack_from('<IH',b,p); p+=6
        s,p=rstr(b,p); d,p=rstr(b,p)
        cls[cid]+=1; pre[str(ref)[:3]]+=1
        out.write(f"{coll}\t{ref}\t{cid}\t{s}\t{d}\n")
    print('collection',coll,'count',n,'lcid',hex(lcid),'classIds',sorted(cls.items()),'refprefix',pre.most_common(12))
print('end',p,len(b))
