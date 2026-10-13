import struct,bisect,sys,collections
D='/home/alex/projects/encarta/extracted/eit/EE/ENCARTA/CATALOG.DLX/catalog2/'
ini={}
for l in open('/home/alex/projects/encarta/extracted/iso/EE/ENCARTA/DELUXE.INI',encoding='latin1'):
    if '={' in l: k,v=l.strip().split('='); ini[v.upper()]=k
guids=open(D+'guid.lst','rb').read().decode('utf-16-le').split('\r\n')
pk=lambda i: ini.get(guids[i].upper(), guids[i] or '<blank>') if i<len(guids) else '?%d'%i
def ecn(fn):
    b=open(D+fn,'rb').read(); assert b[:4]==b'ECN\0'
    N,=struct.unpack_from('<I',b,len(b)-4); assert 8+8*N==len(b)
    recs=[struct.unpack_from('<HH',b,4+4*i) for i in range(N)]
    ids=list(struct.unpack_from('<%dI'%N,b,4+4*N))
    assert ids==sorted(ids)
    return ids,recs
def ecs(fn):
    b=open(D+fn,'rb').read(); assert b[:4]==b'ECS\0'
    N,=struct.unpack_from('<I',b,len(b)-4); W=(len(b)-8-4*N)//N
    recs=[struct.unpack_from('<HH',b,4+4*i) for i in range(N)]
    keys=[b[4+4*N+W*i:4+4*N+W*(i+1)].decode('utf-16-le').rstrip() for i in range(N)]
    return keys,recs,W
for fn in ['content.ecn','data.ecn','relart.ecn']:
    ids,recs=ecn(fn)
    c=collections.Counter(pk(r[0]) for r in recs)
    pre=collections.Counter(str(i)[:3] for i in ids)
    print(fn,'N',len(ids),dict(c), 'idprefix',pre.most_common(8))
    for q in [761575094,761561691,1861582863,1500552,210000001,701704016]:
        j=bisect.bisect_left(ids,q)
        if j<len(ids) and ids[j]==q: print('   ',q,'->',pk(recs[j][0]),recs[j][1])
for fn in ['baggage.ecs','generic.ecs']:
    keys,recs,W=ecs(fn)
    c=collections.Counter(pk(r[0]) for r in recs)
    print(fn,'N',len(keys),'keywidth bytes',W,dict(c))
    for i in list(range(3))+[len(keys)//2, len(keys)-1]: print('   ',keys[i],pk(recs[i][0]),recs[i][1])
print('---- corrected decoding: u8 pkg, u8 localHi, u16 localLo')
for fn in ['content.ecn','data.ecn','relart.ecn']:
    ids,recs=ecn(fn)
    mx=collections.defaultdict(int); cnt=collections.Counter(); hi=0
    for p,l in recs:
        pkg=p&0xff; loc=((p>>8)<<16)|l
        mx[pk(pkg)]=max(mx[pk(pkg)],loc); cnt[pk(pkg)]+=1
    print(fn,{k:(cnt[k],mx[k]) for k in cnt})
    # check uniqueness of (pkg,loc)
    s=set((p&0xff,((p>>8)<<16)|l) for p,l in recs); print('  unique targets',len(s),'of',len(recs))
