'use strict';
const http = require('node:http');
const path = require('node:path');
const app = require(path.resolve(process.argv[2], 'index.js')).handler;
const request = (port, method, route, body, headers = {}) => new Promise((resolve, reject) => {
  const req = http.request({ hostname: '127.0.0.1', port, method, path: route, headers }, res => {
    let raw = '';
    res.on('data', c => { raw += c; });
    res.on('end', () => {
      let json; try { json = JSON.parse(raw); } catch {}
      resolve({ status: res.statusCode, headers: res.headers, json });
    });
  });
  req.setTimeout(5000, () => req.destroy(Error('request timeout')));
  req.on('error', reject);
  if (body !== undefined) req.write(body);
  req.end();
});
const server = app.listen(0, '127.0.0.1', async () => {
  try {
    delete process.env.CORS_ALLOWED_ORIGINS;
    const port = server.address().port;
    const route = '/v1/schema-registry/validate';
    const send = (body, extra = {}) => request(port, 'POST', route, body, { 'Content-Type': 'application/json', ...extra });
    const large = await send(JSON.stringify({ padding: 'x'.repeat(1024 * 1024) }));
    const charset = await send('{}', { 'Content-Type': 'application/json; charset=iso-8859-1' });
    const encoding = await send('{}', { 'Content-Encoding': 'rot13' });
    const malformed = await send('{');
    const valid = await send(JSON.stringify({ schema_type: 'json_schema', schema_content: '{"type":"object"}' }));
    const invalid = await send(JSON.stringify({ schema_type: 'json_schema', schema_content: '[]' }));
    const health = await request(port, 'GET', '/v1/schema-registry/health', undefined, { Origin: 'https://denied.example', 'X-Correlation-ID': 'parser-anchor' });
    const unknown = await request(port, 'GET', '/missing');
    const structured = r => r.headers['content-type']?.includes('application/json')
      && r.json?.execution_metadata?.trace_id === r.headers['x-correlation-id']
      && r.json?.layers_executed?.some(l => l.layer === 'REQUEST_BOUNDARY' && l.status === 'error');
    const holdout = [
      large.status === 413 && large.json?.error === 'Request body exceeds maximum size' && structured(large),
      charset.status === 415 && charset.json?.error === 'Unsupported request charset' && structured(charset),
      encoding.status === 415 && encoding.json?.error === 'Unsupported content encoding' && structured(encoding),
    ];
    const anchors = [
      malformed.status === 400 && malformed.json?.error === 'Request body contains invalid JSON',
      valid.status === 200 && valid.json?.valid === true,
      invalid.status === 422 && invalid.json?.valid === false,
      health.status === 200 && health.json?.status === 'healthy',
      health.headers['access-control-allow-origin'] === undefined,
      unknown.status === 404 && health.json?.execution_metadata?.trace_id === 'parser-anchor',
    ];
    console.log(JSON.stringify({ holdout, anchors, statuses: [large.status, charset.status, encoding.status] }));
    if (process.argv.includes('--assert') && ![...holdout, ...anchors].every(Boolean)) process.exitCode = 1;
  } catch (error) { console.error(error); process.exitCode = 1; }
  finally { server.close(); }
});
