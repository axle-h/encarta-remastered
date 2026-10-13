// Static file server for out/ with HTTP Range support (PMTiles). Usage: node server.js <out dir> <index.html>
const http = require('http'), fs = require('fs'), path = require('path');
const root = path.resolve(process.argv[2]), page = process.argv[3];
http.createServer((req, res) => {
  let u = decodeURIComponent(req.url.split('?')[0]);
  if (u === '/' || u === '/index.html') { res.writeHead(200, {'content-type': 'text/html'}); return res.end(fs.readFileSync(page)); }
  const p = path.join(root, u);
  if (!p.startsWith(root) || !fs.existsSync(p)) { res.writeHead(404); return res.end(); }
  const size = fs.statSync(p).size;
  const type = p.endsWith('.json') ? 'application/json' : p.endsWith('.png') ? 'image/png' : 'application/octet-stream';
  const r = req.headers.range;
  if (r) { const m = /bytes=(\d+)-(\d*)/.exec(r); const s = +m[1], e = Math.min(m[2] ? +m[2] : size - 1, size - 1);
    res.writeHead(206, {'content-range': `bytes ${s}-${e}/${size}`, 'content-length': e - s + 1, 'accept-ranges': 'bytes', 'content-type': type, 'access-control-allow-origin': '*'});
    return fs.createReadStream(p, {start: s, end: e}).pipe(res); }
  res.writeHead(200, {'content-length': size, 'content-type': type, 'accept-ranges': 'bytes'}); fs.createReadStream(p).pipe(res);
}).listen(8099, () => console.log('listening'));
