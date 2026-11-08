import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
const course=JSON.parse(await readFile(new URL('../content/course.json',import.meta.url),'utf8'));
const references=JSON.parse(await readFile(new URL('../content/references.json',import.meta.url),'utf8'));
const activity=(id)=>course.lessons.flatMap(l=>l.activities.map(a=>({...a,lesson:l.id,lesson_version:l.version}))).find(a=>a.id===id);
async function ready(page){await expect(page.getByTestId('worker-status')).toHaveAttribute('data-state','ready');}
async function open(page,id){const a=activity(id);await page.goto(`/nibli-learn/#/lesson/${a.lesson}/activity/${a.id}`);await ready(page);await expect(page.getByRole('heading',{level:1,name:a.title,exact:true})).toBeVisible();return a;}
async function initial(page,family,status){await page.getByTestId('activity-run').click();await ready(page);await expect(page.locator('.operation-results').getByTestId('operation-result')).toHaveAttribute('data-family',family);if(status)await expect(page.locator('.operation-results').getByTestId('operation-result').locator('[data-status]')).toHaveAttribute('data-status',status);}
async function action(page,id,family,status){const button=page.getByTestId(`action-${id}`);await expect(button).toBeEnabled();await button.click();await ready(page);await expect(button).toBeDisabled();if(family)await expect(page.locator('.operation-results').getByTestId('operation-result')).toHaveAttribute('data-family',family);if(status)await expect(page.locator('.operation-results').getByTestId('operation-result').locator('[data-status]')).toHaveAttribute('data-status',status);}
async function saved(page){await expect(page.getByRole('button',{name:'Saved on this device',exact:true})).toBeVisible();return page.evaluate(async()=>{const p=await import('/nibli-learn/platform/platform.js');const r=JSON.parse(await p.loadWorkspace());if(!r.ok||!r.value)throw Error('No workspace');return r.value.data;});}

test('all nineteen units are navigable and the first learning loop records actual evidence',async({page},testInfo)=>{
 const errors=[];page.on('pageerror',e=>errors.push(e.message));await page.goto('/nibli-learn/#/course');await ready(page);
 for(const unit of ['F1','F2','F3','F4','F5','F6','U1','U2','U3','U4','I1','I2','I3','I4','B1','B2','B3','B4','B5'])await expect(page.getByTestId(`unit-${unit}`)).toBeVisible();
 await expect(page.locator('[data-testid^="activity-"]')).toHaveCount(116);
 await page.getByTestId('activity-found.inputs-given').click();await ready(page);
 await page.getByLabel('Your prediction or diagnosis').fill('FALSE: the safety approval is not accepted.');
 await initial(page,'decision','FALSE');await action(page,'admit-approval','decision','TRUE');
 const a=activity('found.inputs-given');await page.getByRole('radio').nth(a.question.correct).check();await page.getByRole('checkbox').check();await page.getByTestId('check-activity').click();await expect(page.getByTestId('check-activity')).toHaveText('✓ Activity checked');
 await page.getByRole('button',{name:'Reveal next hint',exact:true}).click();await page.getByRole('button',{name:'Reveal next hint',exact:true}).click();await expect(page.getByRole('button',{name:'Reveal next hint',exact:true})).toBeDisabled();
 const data=await saved(page);const p=data.progress.lessons[a.lesson][a.lesson_version][a.id][a.version];expect(p.checked).toBe(true);expect(p.evidence).toHaveLength(2);expect(p.evidence.every(e=>e.engine_revision&&e.run_id&&e.epoch&&e.result_family==='decision')).toBe(true);expect(p.actions).toEqual(['admit-approval']);
 await expect.poll(()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);await page.screenshot({path:testInfo.outputPath('foundations-dark.png'),fullPage:true});expect(errors).toEqual([]);
});

test('non-Boolean operations preserve empty, unavailable and actual replay without reexecution',async({page})=>{
 await page.addInitScript(()=>{const W=window.Worker;window.requests=[];window.Worker=class extends W{postMessage(v,...rest){window.requests.push(typeof v==='string'?JSON.parse(v):v);return super.postMessage(v,...rest);}};});
 await open(page,'use.aggregate-results');await initial(page,'aggregated');await expect(page.locator('.operation-results').getByTestId('operation-result')).toContainText('11');
 await action(page,'minimum','aggregated');await action(page,'maximum','aggregated');await action(page,'average','aggregated');await expect(page.locator('.operation-results').getByTestId('operation-result')).toContainText('5.5');
 await action(page,'empty','aggregated');await expect(page.locator('.operation-results').getByTestId('operation-result')).toContainText('Empty');await action(page,'missing-variable','collection_unavailable');await action(page,'repair-variable','aggregated');
 const count=await page.evaluate(()=>window.requests.length);await page.getByRole('tab',{name:'Execution replay',exact:true}).click();await expect(page.getByTestId('replay-next')).toBeEnabled();const position=await page.getByTestId('replay-position').textContent();await page.getByTestId('replay-next').click();await expect(page.getByTestId('replay-position')).not.toHaveText(position);await page.getByRole('tab',{name:'Proof',exact:true}).click();await expect(page.getByText('This operation has no proof certificate')).toBeVisible();expect(await page.evaluate(()=>window.requests.length)).toBe(count);
});

test('negative-cycle refusal preserves sources and inspection has no fabricated chronology',async({page})=>{
 await open(page,'use.negative-cycle');await initial(page,'inspection');await page.getByRole('tab',{name:'Execution replay',exact:true}).click();await expect(page.getByRole('heading',{name:'No execution chronology is available'})).toBeVisible();await page.getByRole('tab',{name:'Result',exact:true}).click();
 const before=await page.locator('.accepted-sources [data-assertion-id]').count();await action(page,'cycle','failed');expect(await page.locator('.accepted-sources [data-assertion-id]').count()).toBe(before);await expect(page.locator('.operation-results').getByTestId('operation-result')).toContainText('change · reasoning');await action(page,'repair-cycle','decision','FALSE');await action(page,'inspect-repair','inspection');
});

test('detached candidate is distinct, apply previews source changes and reruns under main profile',async({page})=>{
 await open(page,'use.detached-what-if');await initial(page,'decision','FALSE');const before=(await saved(page));const base=before.projects[before.active_project].session;
 await action(page,'candidate-review');await expect(page.locator('.candidate-panel')).toContainText('TRUE');expect((await saved(page)).projects[before.active_project].session).toEqual(base);
 await action(page,'check-main','decision','FALSE');await page.getByRole('button',{name:'Preview Apply as real change',exact:true}).click();await expect(page.getByRole('dialog')).toContainText('Candidate IDs and profile are not copied');await page.getByRole('button',{name:'Confirm Apply to main project',exact:true}).click();await ready(page);await expect(page.locator('.candidate-panel')).toHaveCount(0);await expect(page.locator('.operation-results').getByTestId('operation-result').locator('[data-status]')).toHaveAttribute('data-status','TRUE');
 const after=await saved(page);const session=after.projects[after.active_project].session;expect(session.epoch).toBe(base.epoch);expect(session.revision).not.toBe(base.revision);expect(session.profile).toEqual(base.profile);expect(session.assertions.filter(a=>a.active&&a.source==='Proposed reviewer Cart5 approval')).toHaveLength(1);
});

test('reference search compiles actual source and generic reading remains accessible',async({page},testInfo)=>{
 await open(page,'use.shared-witness');await initial(page,'answers');await page.getByRole('button',{name:'Reference',exact:true}).click();await page.getByLabel('Search references').fill('negation');await expect(page.locator('.reference-index nav button')).not.toHaveCount(references.entries.length);await page.getByLabel('KR source').fill('approves(Reviewer, Cart7)');await page.getByRole('button',{name:'Compile and inspect',exact:true}).click();await ready(page);await expect(page.locator('.operation-results').getByTestId('operation-result')).toHaveAttribute('data-family','failed');await page.getByLabel('KR source').fill('approves(Reviewer, Cart7).');await page.getByRole('button',{name:'Compile and inspect',exact:true}).click();await ready(page);await expect(page.locator('.operation-results').getByTestId('operation-result')).toHaveAttribute('data-family','compiled');
 await open(page,'use.incomplete-collection');await initial(page,'collection_unavailable');await page.getByTestId('theme-toggle').click();await page.setViewportSize({width:900,height:1100});await expect.poll(()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);await page.screenshot({path:testInfo.outputPath('use-light-tablet.png'),fullPage:true});await page.setViewportSize({width:390,height:844});await expect(page.getByTestId('activity-run')).toBeHidden();await expect(page.getByRole('heading',{level:1})).toBeVisible();await expect.poll(()=>page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);await page.screenshot({path:testInfo.outputPath('use-light-reading.png'),fullPage:true});
});

test('project export validates in isolation and rejects incompatible engine imports',async({page},testInfo)=>{
 await open(page,'use.identity-count');await initial(page,'counted');const before=await saved(page);await page.getByRole('link',{name:'Local data · no account · no analytics',exact:true}).click();
 const downloadEvent=page.waitForEvent('download');await page.getByRole('button',{name:'Export project',exact:true}).first().click();const file=testInfo.outputPath('project.json');await(await downloadEvent).saveAs(file);const project=JSON.parse(await readFile(file,'utf8'));
 await page.getByLabel('Import type').selectOption('project');await page.getByLabel('Exported JSON').fill(JSON.stringify(project));await page.getByRole('button',{name:'Validate and preview',exact:true}).click();await ready(page);await expect(page.getByRole('heading',{name:'Validated preview',exact:true})).toBeVisible();expect((await saved(page)).projects).toEqual(before.projects);await page.getByRole('button',{name:'Cancel import',exact:true}).click();
 const bad={...project,engine_revision:'another-engine'};await page.getByLabel('Exported JSON').fill(JSON.stringify(bad));await page.getByRole('button',{name:'Validate and preview',exact:true}).click();await expect(page.getByRole('alert')).toContainText('Incompatible');expect((await saved(page)).projects).toEqual(before.projects);
});

for(const id of ['found.argument-order','found.connectives-scope','found.source-mismatch','found.four-outcomes','found.depth-boundary','found.input-versus-outcome','use.places-omissions','use.atomic-replacement','use.profiles-materialization']){
 test(`authored diagnosis and repair: ${id}`,async({page})=>{
  const a=await open(page,id);await initial(page,a.expected.family,a.expected.status);
  for(const step of a.actions)await action(page,step.id,step.expected?.family,step.expected?.status);
  if(a.question)await page.getByRole('radio').nth(a.question.correct).check();
  if(a.self_review.length)await page.getByRole('checkbox').check();
  await page.getByTestId('check-activity').click();await expect(page.getByTestId('check-activity')).toHaveText('✓ Activity checked');
  const data=await saved(page);const p=data.progress.lessons[a.lesson][a.lesson_version][a.id][a.version];expect(p.checked).toBe(true);expect(p.actions).toHaveLength(a.actions.length);
  if(id==='found.input-versus-outcome')expect(p.evidence.some(e=>e.result_family==='failed')||p.expected_failures.some(e=>e.action===null)).toBe(true);
  if(id==='use.atomic-replacement')expect(p.expected_failures.some(e=>e.action==='failed-replacement'&&e.stage==='change'&&e.error_class==='syntax')).toBe(true);
 });
}

test('F6 reflections require explicit self-review and retain learner writing without invented grading',async({page},testInfo)=>{
 const a=await open(page,'found.maintenance-choice');
 const response='The policy owner maintains the requirements; a second reviewer checks each new source. Start with one checkout rule and four counterexamples. Revisit the workflow if review effort outweighs the errors caught.';
 await expect(page.getByTestId('activity-run')).toHaveCount(0);
 await page.getByLabel('Your reasoning and plan (optional)',{exact:true}).fill(response);
 await page.getByTestId('check-activity').click();await expect(page.getByTestId('check-activity')).toHaveText('Check this activity');
 await page.getByRole('checkbox').check();await page.getByTestId('check-activity').click();await expect(page.getByTestId('check-activity')).toHaveText('✓ Activity checked');
 const data=await saved(page);const p=data.progress.lessons[a.lesson][a.lesson_version][a.id][a.version];expect(p.self_reviewed).toBe(true);expect(p.reflection).toBe(response);expect(p.evidence).toEqual([]);expect(p.checked).toBe(true);await expect(page.getByText('Self-review recorded.',{exact:false})).toBeVisible();
 await page.reload();await ready(page);await expect(page.getByLabel('Your reasoning and plan (optional)',{exact:true})).toHaveValue(response);
 await page.getByRole('link',{name:'Local data · no account · no analytics',exact:true}).click();const downloading=page.waitForEvent('download');await page.getByRole('button',{name:'Export progress',exact:true}).click();const file=testInfo.outputPath('reflection-progress.json');await(await downloading).saveAs(file);const exported=JSON.parse(await readFile(file,'utf8'));expect(exported.lessons[a.lesson][a.lesson_version][a.id][a.version].reflection).toBe(response);
});

test('what-if completion requires candidate evidence, while Apply is not a second baseline check',async({page})=>{
 const a=await open(page,'use.detached-what-if');await initial(page,'decision','FALSE');await page.getByRole('checkbox').check();await page.getByTestId('check-activity').click();await expect(page.getByTestId('check-activity')).toHaveText('Check this activity');
 await action(page,'candidate-review');const candidateData=await saved(page);let progress=candidateData.progress.lessons[a.lesson][a.lesson_version][a.id][a.version];expect(progress.candidate_evidence).toHaveLength(1);expect(progress.candidate_evidence[0].action).toBe('candidate-review');expect(progress.evidence).toHaveLength(1);expect(progress.evidence[0].epoch).not.toBe(progress.candidate_evidence[0].epoch);
 await action(page,'check-main','decision','FALSE');await page.getByTestId('check-activity').click();await expect(page.getByTestId('check-activity')).toHaveText('✓ Activity checked');
 await page.getByRole('button',{name:'Preview Apply as real change',exact:true}).click();await expect(page.getByRole('button',{name:'Keep separate',exact:true})).toBeFocused();await page.keyboard.press('Tab');await expect(page.getByRole('button',{name:'Confirm Apply to main project',exact:true})).toBeFocused();await page.keyboard.press('Tab');await expect(page.getByRole('dialog').locator('summary')).toBeFocused();await page.keyboard.press('Escape');await expect(page.getByRole('dialog')).toHaveCount(0);await expect(page.getByRole('button',{name:'Preview Apply as real change',exact:true})).toBeFocused();
 await page.getByRole('button',{name:'Preview Apply as real change',exact:true}).click();await page.getByRole('button',{name:'Confirm Apply to main project',exact:true}).click();await ready(page);await expect(page.locator('.operation-results').getByTestId('operation-result').locator('[data-status]')).toHaveAttribute('data-status','TRUE');progress=(await saved(page)).progress.lessons[a.lesson][a.lesson_version][a.id][a.version];expect(progress.evidence).toHaveLength(2);expect(progress.candidate_evidence).toHaveLength(1);
});

test('offline page reports a development installation accurately and requires activation',async({page})=>{
 await page.goto('/nibli-learn/#/packs');await ready(page);await expect(page.getByRole('heading',{name:'Take the investigation with you.'})).toBeVisible();await expect(page.getByRole('button',{name:'Download and verify',exact:true})).toBeVisible();const development=await page.evaluate(async()=>{const p=await import('/nibli-learn/platform/platform.js');const c=JSON.parse(await p.offlineCatalog());return c.value.catalog.entries.some(e=>e.development===true);});if(development)await expect(page.getByText('Development preview · not a release',{exact:true}).first()).toBeVisible();else await expect(page.getByText('Development preview · not a release',{exact:true})).toHaveCount(0);await expect(page.getByText('No complete active offline installation is confirmed.',{exact:true})).toBeVisible();
});

test('an intervening main change stales a candidate until an actual retest',async({page})=>{
 await open(page,'use.detached-what-if');await initial(page,'decision','FALSE');await action(page,'candidate-review');await page.getByText('Try your own source change',{exact:true}).click();await page.getByLabel('Statement source',{exact:true}).fill('computer(OtherTerminal).');await page.getByLabel('Source label',{exact:true}).fill('Additional inventory observation');await page.getByRole('button',{name:'Admit this source',exact:true}).click();await ready(page);await expect(page.locator('.candidate-panel')).toContainText('Base changed · retest before applying');await expect(page.getByRole('button',{name:'Preview Apply as real change',exact:true})).toBeDisabled();await page.getByRole('button',{name:'Retest against current main inputs',exact:true}).click();await ready(page);await expect(page.getByRole('button',{name:'Preview Apply as real change',exact:true})).toBeEnabled();await action(page,'check-main','decision','FALSE');const addedSource=page.locator('.accepted-sources .assertion-row').filter({hasText:'Additional inventory observation'});await expect(addedSource).toHaveCount(1);await addedSource.getByRole('button',{name:'Withdraw this source',exact:true}).click();await ready(page);await expect(addedSource).toHaveCount(0);await expect(page.getByRole('button',{name:'Preview Apply as real change',exact:true})).toBeDisabled();
});

test('an imported run stays a read-only archive without granting progress',async({page},testInfo)=>{
 await open(page,'use.identity-count');await initial(page,'counted');const before=await saved(page);const downloading=page.waitForEvent('download');await page.getByTestId('operation-export').click();const file=testInfo.outputPath('operation.json');await(await downloading).saveAs(file);const record=JSON.parse(await readFile(file,'utf8'));await page.getByRole('link',{name:'Local data · no account · no analytics',exact:true}).click();await page.getByLabel('Import type').selectOption('archive');await page.getByLabel('Exported JSON').fill(JSON.stringify({...record,schema:999}));await page.getByRole('button',{name:'Validate and preview',exact:true}).click();await expect(page.getByRole('alert')).toContainText('not a supported');await expect(page.getByRole('button',{name:'Apply this validated import',exact:true})).toHaveCount(0);expect((await saved(page)).progress).toEqual(before.progress);await page.getByLabel('Exported JSON').fill(JSON.stringify(record));await page.getByRole('button',{name:'Validate and preview',exact:true}).click();await page.getByRole('button',{name:'Apply this validated import',exact:true}).click();await expect(page.getByRole('heading',{name:'Read-only archived evidence',exact:true})).toBeVisible();const after=await saved(page);expect(after.progress).toEqual(before.progress);expect(after.projects).toEqual(before.projects);await expect(page.locator('.archived-run pre')).toContainText(record.id);
});

test('constraint scans distinguish violations, clean completion and unresolved checks',async({page})=>{
 await open(page,'use.constraint-scan');await initial(page,'contradictions');const result=page.locator('.operation-results').getByTestId('operation-result');await expect(result).toContainText('Completed scan · violations found');await action(page,'repair-disjunction','contradictions');await expect(result).toContainText('Clean completed scan');await action(page,'limited-scan','contradictions');await expect(result).toContainText('Scan has unresolved checks');await action(page,'complete-scan','contradictions');await expect(result).toContainText('Completed scan · violations found');
});
