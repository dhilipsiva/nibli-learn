// Explicit local-only smoke, excluded from hermetic CI. No credentials or private context.
import { chromium } from '@playwright/test';
import { writeFile } from 'node:fs/promises';
const target=process.env.NIBLI_LIVE_ORIGIN || 'http://127.0.0.1:4182';
if(!/^http:\/\/127\.0\.0\.1:\d+$/.test(target)) throw Error('This smoke permits only a loopback companion origin.');
const browser=await chromium.launch({headless:true,executablePath:process.env.BROWSER_EXECUTABLE});
try {
 const page=await browser.newPage(); await page.goto(`${target}/nibli-learn/`);
 const observed=await page.evaluate(async()=>{
  const base='http://127.0.0.1:11434';
  const version=await(await fetch(`${base}/api/version`,{credentials:'omit'})).json();
  const tags=await(await fetch(`${base}/api/tags`,{credentials:'omit'})).json();
  const model=tags.models.find(m=>m.name==='llama3.1:8b'); if(!model) throw Error('Expected local smoke model is not installed.');
  const t=await import('/nibli-learn/platform/tutor.js');
  const run=async(protocol,path,name=model.name)=>{
   const config={protocol,endpoint:base+path,model:name,timeout_ms:120000,max_response_bytes:65536};
   const messages=[{role:'user',content:'This is a fictional learning test. An inspection note says Inspector approved CartB. Briefly explain why approves(Inspector, CartA) does not represent that note. Use at most two sentences.'}];
   const prepared=JSON.parse(t.prepareModel(JSON.stringify(config),JSON.stringify(messages))); if(!prepared.ok)throw Error(JSON.stringify(prepared));
   const began=performance.now();const result=JSON.parse(await t.sendModel(JSON.stringify(prepared.preview),'',`local-smoke-${crypto.randomUUID()}`));
   return {protocol,endpoint:config.endpoint,request:JSON.parse(prepared.preview.body),elapsed_ms:performance.now()-began,result};
  };
  const results=[];results.push(await run('ollama','/api/chat')); results.push(await run('openai_chat','/v1/chat/completions')); results.push(await run('ollama','/api/chat','nibli-learn-model-that-is-not-installed'));
  return {schema:1,kind:'local-model-connection-observations',observed_at:new Date().toISOString(),origin:location.origin,version,model:{name:model.name,digest:model.digest},results,limitations:['Local loopback preview only.','Production site origin is not permitted by the current local service configuration.','No hosted provider/account, credential or billing behavior exercised.','Connectivity and complete text responses do not establish semantic accuracy.']};
 });
 await writeFile(process.env.NIBLI_LIVE_REPORT || '/tmp/nibli-learn-phase3-live-model.json',JSON.stringify(observed,null,2)+'\n');
 if(!observed.results.slice(0,2).every(r=>r.result.ok && r.result.reply.text.length>0))throw Error('A live protocol did not complete; inspect report.');
 if(observed.results[2].result.error?.kind!=='unavailable')throw Error('Missing model did not produce the expected failure.');
 console.log(JSON.stringify({origin:observed.origin,version:observed.version,model:observed.model,results:observed.results.map(r=>({protocol:r.protocol,elapsed_ms:r.elapsed_ms,ok:r.result.ok,error:r.result.error?.kind,bytes:r.result.reply?.bytes}))},null,2));
} finally {await browser.close();}
