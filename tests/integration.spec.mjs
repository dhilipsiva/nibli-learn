import { test, expect } from '@playwright/test';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';

// Actual HTTP traffic through Chromium, with a deterministic local provider.
let server, origin, requests;
test.beforeAll(async () => {
  requests = [];
  server = createServer(async (req, res) => {
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Headers', 'content-type,authorization');
    if (req.method === 'OPTIONS') { res.end(); return; }
    let raw = ''; for await (const chunk of req) raw += chunk;
    requests.push({ path: req.url, headers: req.headers, body: JSON.parse(raw || '{}') });
    const name = req.url.slice(1);
    if (/^http\d+$/.test(name)) { res.statusCode = Number(name.slice(4)); res.end(`secret error ${req.headers.authorization}`); return; }
    if (name === 'redirect') { res.writeHead(307, { location: `${origin}/leak` }); res.end(); return; }
    if (name === 'disconnect') { req.socket.destroy(); return; }
    if (name === 'slow') { res.setHeader('Content-Type', 'application/json'); res.write('{"message":{"content":"partial'); setTimeout(() => res.end('"},"done":true}'), 2500); return; }
    if (name === 'malformed') { res.end('not JSON'); return; }
    if (name === 'utf8') { res.end(Buffer.from([0xff, 0xfe])); return; }
    if (name === 'big') { res.end(' '.repeat(5000)); return; }
    const text = name === 'reflect' ? `${req.headers.authorization} approves(Inspector, CartB).` : 'approves(Inspector, CartB).';
    const message = { role: 'assistant', content: text };
    if (name === 'tools') message.tool_calls = [{ function: { name: 'admit' } }];
    res.setHeader('Content-Type', 'application/json');
    res.end(JSON.stringify(name === 'chat' ? { choices: [{ message, finish_reason: 'stop' }] } : { message, done: name !== 'incomplete', done_reason: name === 'truncated' ? 'length' : 'stop' }));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  origin = `http://127.0.0.1:${server.address().port}`;
});
test.afterAll(async () => { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); });
async function ready(page) { await expect(page.getByTestId('worker-status')).toHaveAttribute('data-state', 'ready'); }
async function saved(page) { await expect(page.getByRole('button', {name:'Saved on this device',exact:true})).toBeVisible(); return page.evaluate(async () => JSON.parse(await (await import('/nibli-learn/platform/platform.js')).loadWorkspace()).value.data); }
async function open(page) { await page.goto('/nibli-learn/#/lesson/found.prediction-derivation/activity/found.inputs-given'); await ready(page); }
async function transport(page, path, options = {}) {
  return page.evaluate(async ({url, options}) => {
    const t = await import('/nibli-learn/platform/tutor.js');
    const config = { protocol: 'ollama', endpoint: url, model: 'fixture-model', timeout_ms: 5000, max_response_bytes: 4096, ...options };
    const prepared = JSON.parse(t.prepareModel(JSON.stringify(config), JSON.stringify([{role:'user', content:'Explain the fictional inspection note.'}])));
    if (!prepared.ok) return prepared;
    return JSON.parse(await t.sendModel(JSON.stringify(prepared.preview), 'test-key-canary', `test-${crypto.randomUUID()}`));
  }, {url:`${origin}/${path}`, options});
}

test('model transport sends only previewed text, bounds failures and rejects redirects and tool calls', async ({page, context}) => {
  await open(page);
  await context.addCookies([{name:'ambient', value:'must-not-send', url:origin}]);
  for (const [path, protocol] of [['native','ollama'],['chat','openai_chat']]) {
    const result = await transport(page,path,{protocol}); expect(result.ok).toBe(true); expect(result.reply.text).toBe('approves(Inspector, CartB).');
    const call = requests.at(-1); expect(call.headers.authorization).toBe('Bearer test-key-canary'); expect(call.headers.cookie).toBeUndefined(); expect(call.headers.referer).toBeUndefined();
    expect(call.body.messages).toEqual([{role:'user',content:'Explain the fictional inspection note.'}]); expect(call.body.stream).toBe(false);
    if (protocol==='openai_chat') expect(call.body.store).toBe(false);
  }
  const cases = { http401:'authentication', http429:'usage_limit', http404:'unavailable', http500:'provider_error', malformed:'malformed', utf8:'malformed', tools:'rejected', incomplete:'malformed', big:'response_limit', redirect:'network', disconnect:'network' };
  for (const [path, kind] of Object.entries(cases)) {
    const result=await transport(page,path,{max_response_bytes:1024}); expect(result.ok,path).toBe(false); expect(result.error.kind,path).toBe(kind); expect(JSON.stringify(result)).not.toContain('test-key-canary'); expect(result.reply).toBeUndefined();
  }
  expect(requests.some(r=>r.path==='/leak')).toBe(false);
  const reflected=await transport(page,'reflect'); expect(reflected.reply.text).toContain('[credential redacted]'); expect(reflected.reply.text).not.toContain('test-key-canary');
  expect((await transport(page,'truncated')).reply.truncated).toBe(true);
  expect((await transport(page,'slow',{timeout_ms:100})).error.kind).toBe('timeout');
  const cancelled = await page.evaluate(async url => {
    const t=await import('/nibli-learn/platform/tutor.js'); const preview=JSON.parse(t.prepareModel(JSON.stringify({protocol:'ollama',endpoint:url,model:'fixture',timeout_ms:5000,max_response_bytes:4096}),JSON.stringify([{role:'user',content:'cancel me'}]))).preview;
    const pending=t.sendModel(JSON.stringify(preview),'','cancel-test'); setTimeout(()=>t.cancelModel('cancel-test'),100); return JSON.parse(await pending);
  }, `${origin}/slow`);
  expect(cancelled.error.kind).toBe('cancelled'); expect(cancelled.reply).toBeUndefined();
  const before=requests.length;
  for(const endpoint of ['http://example.com/chat', 'https://key:secret@example.com/chat', 'https://example.com/chat?key=secret']) expect((await transport(page,'native',{endpoint})).error.kind).toBe('configuration');
  expect(requests).toHaveLength(before);
});

test('review remains an explicit human decision, edits invalidate it and provenance survives reload', async ({page}) => {
  await open(page);
  const original = await saved(page); const base = original.projects[original.active_project].session;
  await page.getByText('Review an application draft',{exact:true}).click();
  await page.getByRole('button',{name:'Prepare readings and admission check',exact:true}).click(); await ready(page);
  await expect(page.locator('.review-panel')).toContainText('Deterministic gates and detached admission passed');
  expect((await saved(page)).projects[original.active_project].session).toEqual(base);
  const admit=page.getByRole('button',{name:'Admit reviewed draft to main project',exact:true}); await expect(admit).toBeDisabled();
  await page.getByRole('button',{name:'Reject this draft',exact:true}).click();
  await page.getByLabel('Formal draft for review',{exact:true}).fill('approves(Inspector, CartB).');
  await page.getByRole('button',{name:'Prepare readings and admission check',exact:true}).click(); await ready(page);
  await page.getByLabel('I reviewed the source, roles, scope and negation; I approve this exact draft for admission.').check(); await expect(admit).toBeEnabled();
  await page.getByLabel('Original source for review',{exact:true}).fill('Inspector approved CartB after a second review.'); await expect(admit).toHaveCount(0);
  await page.getByRole('button',{name:'Prepare readings and admission check',exact:true}).click(); await ready(page);
  await page.getByLabel('I reviewed the source, roles, scope and negation; I approve this exact draft for admission.').check(); await admit.click(); await ready(page);
  const stored=await saved(page); const after=stored.projects[stored.active_project];
  expect(after.session.revision).not.toBe(base.revision); const row=after.session.assertions.find(a=>a.review);
  expect(row.text).toBe('approves(Inspector, CartB).'); expect(row.review.source_text).toBe('Inspector approved CartB after a second review.');
  expect(JSON.stringify(after)).not.toContain('"prepared"'); expect(JSON.stringify(after)).not.toContain('"human_confirmed"');
  await page.getByLabel('Formal draft for review',{exact:true}).fill(''); await saved(page); await page.reload(); await ready(page);
  await page.getByText('Review an application draft',{exact:true}).click(); await expect(page.getByLabel('Formal draft for review',{exact:true})).toHaveValue(''); await expect(admit).toHaveCount(0);
  const restored=await saved(page); expect(restored.projects[restored.active_project].session.assertions.find(a=>a.review)).toEqual(row);
});

test('tutor previews before sending, never admits or grades, and excludes keys and replies from exports', async ({page}, testInfo) => {
  await open(page); const before=await saved(page); const count=requests.length;
  await page.locator('#tutor-trigger').click();
  await page.getByLabel('Chat endpoint URL',{exact:true}).fill(`${origin}/native`);
  await page.getByLabel('Model name',{exact:true}).fill('fixture-model');
  await page.getByLabel('API key (optional; memory only)',{exact:true}).fill('ui-secret-canary');
  await page.getByLabel('Question for the optional tutor',{exact:true}).fill('Suggest a formal draft for the fictional inspection.');
  await page.getByRole('button',{name:'Preview exact request',exact:true}).click();
  await expect(page.locator('.context-preview')).not.toContainText('ui-secret-canary');
  await expect(page.locator('.context-preview')).not.toContainText('inspection-note-v1');
  expect(requests).toHaveLength(count);
  await page.getByLabel('Include the review source and formal draft',{exact:true}).check(); await expect(page.getByRole('button',{name:'Send this exact context',exact:true})).toHaveCount(0);
  await page.getByRole('button',{name:'Preview exact request',exact:true}).click(); await expect(page.locator('.context-preview')).toContainText('inspection-note-v1');
  await page.getByRole('button',{name:'Send this exact context',exact:true}).click(); await expect(page.locator('.model-reply')).toContainText('approves(Inspector, CartB).');
  expect(requests).toHaveLength(count+1); expect((await saved(page)).projects).toEqual(before.projects); expect((await saved(page)).progress).toEqual(before.progress);
  const snapshot=JSON.stringify(await saved(page)); expect(snapshot).not.toContain('ui-secret-canary'); expect(snapshot).not.toContain('fixture-model');
  await page.screenshot({path:testInfo.outputPath('tutor-preview-and-response.png'),fullPage:true});
  await page.getByRole('button',{name:'Copy suggestion to review editor',exact:true}).click(); await expect(page.getByLabel('Formal draft for review',{exact:true})).toHaveValue('approves(Inspector, CartB).');
  expect((await saved(page)).projects[before.active_project].session).toEqual(before.projects[before.active_project].session);
  await page.getByRole('link',{name:'Local data · no account · no analytics',exact:true}).click(); const downloading=page.waitForEvent('download'); await page.getByRole('button',{name:'Export project',exact:true}).first().click(); const file=testInfo.outputPath('review-project.json'); await(await downloading).saveAs(file); const exportText=await readFile(file,'utf8'); expect(exportText).not.toContain('ui-secret-canary'); expect(exportText).not.toContain('fixture-model');
  await page.reload(); await ready(page); await page.locator('#tutor-trigger').click(); await expect(page.getByLabel('API key (optional; memory only)',{exact:true})).toHaveValue(''); await expect(page.locator('.model-reply')).toHaveCount(0);
  await page.getByRole('button',{name:'Close side panel',exact:true}).click(); await expect(page.locator('#tutor-trigger')).toBeFocused();
});

test('closing the tutor aborts a pending request and a stale preview cannot be sent', async ({page}) => {
  await open(page); const before=await saved(page);
  await page.locator('#tutor-trigger').click();
  await page.getByLabel('Chat endpoint URL',{exact:true}).fill(`${origin}/slow`);
  await page.getByLabel('Model name',{exact:true}).fill('fixture-model');
  await page.getByLabel('Question for the optional tutor',{exact:true}).fill('Explain this fictional source.');
  await page.getByRole('button',{name:'Preview exact request',exact:true}).click();
  await page.getByRole('button',{name:'Send this exact context',exact:true}).click(); await expect(page.getByRole('button',{name:'Cancel model request',exact:true})).toBeVisible();
  await page.getByRole('button',{name:'Close side panel',exact:true}).click(); await expect(page.locator('#tutor-trigger')).toBeFocused();
  await page.locator('#tutor-trigger').click(); await expect(page.locator('.tutor-panel')).toContainText('Request cancelled'); await expect(page.locator('.model-reply')).toHaveCount(0);
  expect((await saved(page)).projects).toEqual(before.projects);
  await page.getByRole('button',{name:'Close side panel',exact:true}).click();
  await page.getByText('Try your own source change',{exact:true}).click();
  await page.getByLabel('Statement source',{exact:true}).fill('computer(OtherTerminal).');
  await page.getByLabel('Source label',{exact:true}).fill('Intervening terminal observation'); await page.getByRole('button',{name:'Admit this source',exact:true}).click(); await ready(page);
  await page.locator('#tutor-trigger').click(); await expect(page.getByRole('button',{name:'Send this exact context',exact:true})).toBeDisabled();
  await page.getByRole('button',{name:'Preview exact request',exact:true}).click(); await expect(page.getByRole('button',{name:'Send this exact context',exact:true})).toBeEnabled();
  await page.keyboard.press('Escape'); await expect(page.locator('#tutor-trigger')).toBeFocused();
});
