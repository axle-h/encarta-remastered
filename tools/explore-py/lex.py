# Megalith *.LEX word-list decoder (exploration; spec: docs/formats/lex.md).
# usage: python3 -I lex.py FILE.LEX [out.txt]
import struct, sys

def load(fn):
    b = open(fn, 'rb').read()
    assert b[:4] == b'\x20\xf3\x9f\x77'
    n, = struct.unpack_from('<I', b, 0x1c)
    body = 0x20 + 2 * n + 4
    u = lambda o: struct.unpack_from('<I', b, body + o)[0]
    hdr = dict(maxlen=struct.unpack_from('<H', b, body + 8)[0], maxarcs=struct.unpack_from('<H', b, body + 10)[0],
               words=u(0x14), nskip=u(0x18), nsym=u(0x1c), nnode=u(0x24), soff=u(0x28), slen=u(0x2c))
    desc = [struct.unpack_from('<H', b, body + 0x30 + 4 * i)[0] for i in range(3)]  # direct counts: sym, node, skip
    p = body + 0x3c
    sym = [struct.unpack_from('<HH', b, p + 4 * i) for i in range(hdr['nsym'])]; p += 4 * hdr['nsym']
    nodes = list(struct.unpack_from('<%dI' % hdr['nnode'], b, p)); p += 4 * hdr['nnode']
    skip = list(struct.unpack_from('<%dI' % hdr['nskip'], b, p)); p += 4 * hdr['nskip']
    assert p == body + hdr['soff'] and p + hdr['slen'] == len(b)
    return hdr, desc, sym, nodes, skip, b[p:]

def words(fn):
    hdr, desc, sym, nodes, skip, S = load(fn)
    def code(k, p):
        c = S[p]
        return (c, p + 1) if c < desc[k] else (desc[k] + (c - desc[k]) * 256 + S[p + 1], p + 2)
    def node(p):
        arcs, cursor, cont = [], None, []
        while True:
            c, p = code(0, p); ch, f = sym[c]; child = None
            if f & 0x20: x, p = code(1, p); cont.append(nodes[x])
            if f & 0x10:
                if f & 0x40: child = -1
                elif f & 0x80: x, p = code(1, p); child = nodes[x]
                elif f & 0x100: child = int.from_bytes(S[p:p + 3], 'big'); p += 3
                else:
                    x, p = code(2, p)
                    if cursor is None: cursor = p
                    cursor += skip[x]; child = cursor
            arcs.append((chr(ch), f, child))
            if f & 2: break
        return [(ch, f, p if c == -1 else c) for ch, f, c in arcs], cont
    out, stack = [], [(0, '')]
    while stack:
        p, pre = stack.pop()
        arcs, cont = node(p)
        stack += [(c, pre) for c in cont]
        for ch, f, child in arcs:
            if f & 1: out.append(pre + ch)
            if f & 0x10: stack.append((child, pre + ch))
    return hdr, out

if __name__ == '__main__':
    hdr, w = words(sys.argv[1])
    print(hdr, 'decoded', len(w), 'distinct', len(set(w)), 'maxlen', max(map(len, w)))
    if len(sys.argv) > 2: open(sys.argv[2], 'w', encoding='utf-8').write('\n'.join(sorted(w)) + '\n')
