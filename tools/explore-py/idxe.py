import sys, struct, collections
def rd7(b, p):
    n=0; s=0
    while True:
        c=b[p]; p+=1; n|=(c&0x7f)<<s; s+=7
        if c<0x80: return n,p
def parse(path, limit=None, out=None):
    b=open(path,'rb').read()
    l,p=rd7(b,0); p+=l
    ver=b[p:p+3]; p+=3
    dual = ver[1]>=7
    p=(p+15)//16*16
    types=collections.Counter(); n=0
    while p + 6 < len(b):
        start=p
        t=b[p]; p+=1
        ref,=struct.unpack_from('<i',b,p); p+=4
        dupe=off=None
        if t&0x40: dupe,=struct.unpack_from('<i',b,p); p+=4
        if t&0x20: off,=struct.unpack_from('<I',b,p); p+=4
        wc=b[p]; p+=1
        l,p=rd7(b,p); txt=b[p:p+l].decode('utf8','replace'); p+=l
        np_=b[p]; p+=1
        parents=list(struct.unpack_from('<%di'%np_,b,p)); p+=4*np_
        seq=None
        if dual: seq,=struct.unpack_from('<I',b,p); p+=4
        types[t&0x1f]+=1; n+=1
        if out: out.write(f"{start//16}\t{t:#04x}\t{ref}\t{dupe}\t{off}\t{wc}\t{txt.replace(chr(9)," ").replace(chr(10)," ").replace(chr(13)," ")}\t{parents}\t{seq}\n")
        p=(p+15)//16*16
        if limit and n>=limit: break
    print(path, 'version', list(ver), 'records', n, 'types', sorted(types.items()), file=sys.stderr)
if __name__=='__main__':
    parse(sys.argv[1], out=open(sys.argv[2],'w') if len(sys.argv)>2 else None)
