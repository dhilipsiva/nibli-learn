import { test, expect } from '@playwright/test';

const claim = 'ready(BuildA, Release).';
const policy = 'all $build: approves(TestRunner, $build) & approves(Reviewer, $build) -> ready($build, Release).';
const initial = [
  { text: 'derived_only("ready").', source: 'Protected readiness declaration' },
  { text: policy, source: 'Release policy' },
  { text: 'approves(TestRunner, BuildA).', source: 'Automated check A1' },
];
const settings = { depth: 10, recording: 'full' };

async function start(page, generation = '1') {
  await page.goto('/nibli-learn/');
  await page.evaluate((generation) => {
    const worker = new Worker('/nibli-learn/worker/bootstrap.js', { type: 'module' });
    let next = 0;
    const pending = new Map();
    const phases = [];
    const completed = [];
    let session;
    worker.onmessage = ({ data }) => {
      const response = typeof data === 'string' ? JSON.parse(data) : data;
      if (response.generation !== generation) return;
      if (response.kind === 'phase') {
        phases.push(response);
        if (globalThis.reasoner.cancelOnPhase === response.phase && pending.has(response.id)) {
          globalThis.reasoner.interruptedPhase = response;
          globalThis.reasoner.terminate();
        }
        return;
      }
      const callback = pending.get(response.id);
      if (!callback) return;
      pending.delete(response.id);
      clearTimeout(callback.timer);
      completed.push(response);
      if (response.result?.Ok) session = response.result.Ok.session;
      callback.resolve(response);
    };
    worker.onerror = (event) => {
      for (const callback of pending.values()) {
        clearTimeout(callback.timer);
        callback.reject(new Error(event.message));
      }
      pending.clear();
    };
    globalThis.reasoner = {
      worker,
      phases,
      completed,
      cancelOnPhase: null,
      interruptedPhase: null,
      session: () => session,
      send(command, expectedRevision, expectedEpoch) {
        const id = `browser-test-${++next}`;
        const expected_revision = expectedRevision !== undefined ? expectedRevision
          : ['initialize', 'restore'].includes(command.kind) ? null : session?.revision ?? null;
        const expected_epoch = expectedEpoch !== undefined ? expectedEpoch
          : ['initialize', 'restore'].includes(command.kind) ? null : session?.epoch ?? null;
        return new Promise((resolve, reject) => {
          const timer = setTimeout(() => { pending.delete(id); reject(new Error(`Worker request ${id} timed out`)); }, 30_000);
          pending.set(id, { resolve, reject, timer });
          worker.postMessage(JSON.stringify({ id, generation, expected_epoch, expected_revision, command }));
        });
      },
      terminate() {
        worker.terminate();
        for (const callback of pending.values()) {
          clearTimeout(callback.timer);
          callback.reject(new Error('Worker terminated'));
        }
        pending.clear();
      },
    };
  }, generation);
}

async function request(page, command, expectedRevision, expectedEpoch) {
  return page.evaluate(({ command, expectedRevision, expectedEpoch }) => globalThis.reasoner.send(command, expectedRevision, expectedEpoch), { command, expectedRevision, expectedEpoch });
}
async function ok(page, command, expectedRevision) {
  const response = await request(page, command, expectedRevision);
  if (response.result.Err) throw new Error(`${response.result.Err.kind}: ${response.result.Err.message}`);
  expect(response.result.Ok).toBeDefined();
  return response.result.Ok;
}
async function initialize(page) {
  return ok(page, { kind: 'initialize', epoch: 'release-test-epoch', statements: initial, settings });
}
async function change(page, changes) {
  return ok(page, { kind: 'change', changes });
}
async function run(page, expected) {
  const reply = await ok(page, { kind: 'run', claim });
  expect(reply.run.outcome.status).toBe(expected);
  expect(reply.run.epoch).toBe(reply.session.epoch);
  expect(reply.run.revision).toBe(reply.session.revision);
  expect(Array.isArray(reply.run.record.events)).toBe(true);
  expect(reply.run.record.events.length).toBeGreaterThan(0);
  return reply;
}

test('actual worker completes the seven-step reasoning and policy-repair controls', async ({ page }) => {
  await start(page);
  await initialize(page);
  const first = await run(page, 'FALSE');
  expect(first.run.outcome.cwa_false).toBe(true);
  await change(page, [{ kind: 'assert', text: 'approves(Reviewer, BuildB).', source: 'Other build review' }]);
  await run(page, 'FALSE');
  const supported = await change(page, [{ kind: 'assert', text: 'approves(Reviewer, BuildA).', source: 'Review A1' }]);
  const firstSupport = supported.session.assertions.find(row => row.source === 'Review A1').id;
  const proofRun = await run(page, 'TRUE');
  expect(proofRun.run.proof_text.length).toBeGreaterThan(0);
  expect(proofRun.run.record).not.toEqual(proofRun.run.proof);
  expect(proofRun.run.record.status).toBe('complete');
  expect(proofRun.run.record.artifacts.some(artifact => artifact.source === claim)).toBe(true);
  expect(proofRun.run.record.initial_snapshot.assertions.length).toBeGreaterThan(0);
  expect(proofRun.run.record.events.some(event => event.phase === 'probe')).toBe(true);
  expect(proofRun.run.record.events.some(event => event.phase === 'proof')).toBe(true);
  for (const event of proofRun.run.record.events) {
    expect(typeof event.sequence).toBe('string');
    expect(event.assertion_ids.every(id => typeof id === 'string')).toBe(true);
  }

  const duplicate = await change(page, [{ kind: 'assert', text: 'approves(Reviewer, BuildA).', source: 'Independent review A2' }]);
  const secondSupport = duplicate.session.assertions.find(row => row.source === 'Independent review A2').id;
  expect(secondSupport).not.toBe(firstSupport);
  expect(typeof firstSupport).toBe('string');
  expect(typeof secondSupport).toBe('string');
  await change(page, [{ kind: 'withdraw', id: firstSupport }]);
  await run(page, 'TRUE');
  await change(page, [{ kind: 'withdraw', id: secondSupport }]);
  const lastSupportRemoved = await run(page, 'FALSE');

  const refusal = await request(page, { kind: 'change', changes: [{ kind: 'assert', text: claim, source: 'Direct outcome shortcut' }] });
  expect(refusal.result.Err).toBeDefined();
  expect(refusal.result.Err.message).toMatch(/derived|direct|protected|ready/i);
  const afterRefusal = await ok(page, { kind: 'inspect' });
  expect(afterRefusal.session.revision).toBe(lastSupportRemoved.session.revision);
  expect(afterRefusal.session.assertions).toEqual(lastSupportRemoved.session.assertions);

  const weak = await ok(page, { kind: 'check_repair', policy: policy.replace(' & ', ' | ') });
  expect(weak.checks).toHaveLength(4);
  expect(weak.checks.some(row => !row.passed)).toBe(true);
  const repaired = await ok(page, { kind: 'check_repair', policy });
  expect(repaired.checks).toHaveLength(4);
  expect(repaired.checks.every(row => row.passed)).toBe(true);
  expect(repaired.session.assertions).toEqual(afterRefusal.session.assertions);
});

test('atomic changes and stale revisions preserve the last committed session', async ({ page }) => {
  await start(page);
  const before = await initialize(page);
  const failed = await request(page, { kind: 'change', changes: [
    { kind: 'assert', text: 'approves(Reviewer, BuildA).', source: 'Would succeed alone' },
    { kind: 'assert', text: 'all $broken:', source: 'Rejected input' },
  ] });
  expect(failed.result.Err).toBeDefined();
  const after = await ok(page, { kind: 'inspect' });
  expect(after.session).toEqual(before.session);
  const changed = await change(page, [{ kind: 'assert', text: 'approves(Reviewer, BuildA).', source: 'Valid review' }]);
  const stale = await request(page, { kind: 'run', claim }, before.session.revision);
  expect(stale.result.Err).toBeDefined();
  expect(stale.result.Err.message).toMatch(/revision|stale/i);
  const unchanged = await ok(page, { kind: 'inspect' });
  expect(unchanged.session).toEqual(changed.session);
  await run(page, 'TRUE');
});

test('a reused revision and numeric ID cannot authorize a withdrawal from an earlier epoch', async ({ page }) => {
  await start(page);
  const original = await initialize(page);
  const id = original.session.assertions.find(row => row.source === 'Automated check A1').id;
  const replacement = await ok(page, { kind: 'initialize', epoch: 'replacement-epoch', statements: initial, settings });
  expect(replacement.session.revision).toBe(original.session.revision);
  expect(replacement.session.assertions.find(row => row.source === 'Automated check A1').id).toBe(id);
  const stale = await request(page, { kind: 'change', changes: [{ kind: 'withdraw', id }] }, original.session.revision, original.session.epoch);
  expect(stale.result.Err).toBeDefined();
  expect(stale.result.Err.kind).toBe('stale');
  const after = await ok(page, { kind: 'inspect' });
  expect(after.session).toEqual(replacement.session);
});

test('terminating an unacknowledged real mutation restores only the last committed image', async ({ page }) => {
  await start(page);
  await initialize(page);
  const committed = await change(page, [{ kind: 'assert', text: 'approves(Reviewer, BuildA).', source: 'Committed review' }]);
  const cancelled = await page.evaluate(async () => {
    let completion = 'pending';
    const changes = Array.from({ length: 4000 }, (_, index) => ({
      kind: 'assert', text: 'dog(Adam).', source: `Unacknowledged batch ${index}`,
    }));
    const pending = globalThis.reasoner.send({ kind: 'change', changes }).then(
      () => { completion = 'completed'; },
      () => { completion = 'terminated'; },
    );
    // A warm real WASM worker receives a substantive mutation. This establishes
    // the pending request boundary; it does not identify an internal search phase.
    await new Promise(resolve => setTimeout(resolve, 20));
    const beforeTermination = completion;
    globalThis.reasoner.terminate();
    await pending;
    return { beforeTermination, completion };
  });
  expect(cancelled).toEqual({ beforeTermination: 'pending', completion: 'terminated' });
  await start(page, '2');
  const restored = await ok(page, { kind: 'restore', session: committed.session });
  expect(restored.session.assertions).toEqual(committed.session.assertions);
  expect(restored.session.assertions.some(row => row.source.startsWith('Unacknowledged batch'))).toBe(false);
  await run(page, 'TRUE');
  const refused = await request(page, { kind: 'change', changes: [{ kind: 'assert', text: claim, source: 'Protection must survive recovery' }] });
  expect(refused.result.Err).toBeDefined();
});

test('a malformed recovery image fails closed without replacing knowledge', async ({ page }) => {
  await start(page);
  const before = await initialize(page);
  const malformed = structuredClone(before.session);
  malformed.recovery = { schema: 999999, invalid: true };
  const response = await request(page, { kind: 'restore', session: malformed });
  expect(response.result.Err).toBeDefined();
  const after = await ok(page, { kind: 'inspect' });
  expect(after.session.assertions).toEqual(before.session.assertions);
  await run(page, 'FALSE');
});

test('unknown, depth limits and explicit negative assertions retain their engine meanings', async ({ page }) => {
  await start(page);
  await initialize(page);
  const unknown = await ok(page, { kind: 'run', claim: 'sum(Adam, 2, 3).' });
  expect(unknown.run.outcome.status).toBe('UNKNOWN');
  expect(unknown.run.outcome.detail).toBeTruthy();
  const negative = await change(page, [{ kind: 'assert', text: '~approves(Observer, BuildC).', source: 'Explicit negative review' }]);
  const row = negative.session.assertions.find(row => row.source === 'Explicit negative review');
  expect(row.active).toBe(true);
  expect(row.text).toBe('~approves(Observer, BuildC).');
  const negativeRun = await ok(page, { kind: 'run', claim: row.text });
  expect(negativeRun.run.outcome.status).toBe('TRUE');
  expect(negativeRun.run.outcome.cwa_false).toBe(false);

  await ok(page, { kind: 'initialize', epoch: 'depth-boundary', settings: { depth: 1, recording: 'full' }, statements: [
    { text: 'dog(Adam).', source: 'Initial evidence' },
    { text: 'animal(every dog).', source: 'First rule' },
    { text: 'alive(every animal).', source: 'Second rule' },
  ] });
  const limited = await ok(page, { kind: 'run', claim: 'alive(Adam).' });
  expect(limited.run.outcome.status).toBe('RESOURCE_EXCEEDED');
  expect(limited.run.outcome.detail).toMatch(/depth/i);
  expect(limited.run.record.profile.max_chain_depth).toBe(1);
  await ok(page, { kind: 'settings', settings: { depth: 20, recording: 'full' } });
  const completed = await ok(page, { kind: 'run', claim: 'alive(Adam).' });
  expect(completed.run.outcome.status).toBe('TRUE');
  expect(completed.run.revision).not.toBe(limited.run.revision);
  expect(limited.run.settings.depth).toBe(1);
});

for (const phase of ['compile', 'probe', 'proof']) {
  test(`termination at the actual ${phase} phase restores the last acknowledged knowledge`, async ({ page }) => {
    test.setTimeout(120_000);
    await start(page);
    await initialize(page);
    const changes = [
      { kind: 'assert', text: 'approves(Reviewer, BuildA).', source: 'Committed review' },
      { kind: 'assert', text: 'animal(every dog).', source: 'Committed animal rule' },
      { kind: 'assert', text: 'alive(every animal).', source: 'Committed living rule' },
      ...Array.from({ length: 1400 }, (_, index) => ({ kind: 'assert', text: 'dog(Adam).', source: `Committed support ${index}` })),
    ];
    const committed = await change(page, changes);
    const interrupted = await page.evaluate(async phase => {
      const reasoner = globalThis.reasoner;
      reasoner.cancelOnPhase = phase;
      const countBefore = reasoner.completed.length;
      const command = phase === 'compile'
        ? { kind: 'change', changes: Array.from({ length: 4000 }, (_, index) => ({ kind: 'assert', text: 'cat(Bela).', source: `Unacknowledged candidate ${index}` })) }
        : { kind: 'run', claim: 'alive(Adam).' };
      let rejected = false;
      try { await reasoner.send(command); }
      catch { rejected = true; }
      return { rejected, observed: reasoner.interruptedPhase, acknowledged: reasoner.completed.length - countBefore };
    }, phase);
    expect(interrupted.rejected).toBe(true);
    expect(interrupted.observed.phase).toBe(phase);
    expect(interrupted.acknowledged).toBe(0);
    await start(page, '2');
    const restored = await ok(page, { kind: 'restore', session: committed.session });
    expect(restored.session.assertions).toEqual(committed.session.assertions);
    expect(restored.session.recovery).toEqual(committed.session.recovery);
    expect(restored.session.assertions.some(row => row.source.startsWith('Unacknowledged candidate'))).toBe(false);
    await run(page, 'TRUE');
  });
}

test('a bounded recording can truncate without changing an admitted batch or logical outcome', async ({ page }) => {
  test.setTimeout(120_000);
  await start(page);
  await ok(page, { kind: 'initialize', epoch: 'bounded-recording', statements: initial,
    settings: { depth: 10, recording: 'standard' } });
  const statements = Array.from({ length: 2500 }, (_, index) => ({
    kind: 'assert', text: 'dog(Adam).', source: `Bounded recording support ${index}`,
  }));
  const admitted = await change(page, statements);
  expect(admitted.change_record.status).toBe('truncated');
  expect(admitted.change_record.truncation_reason).toBeTruthy();
  expect(admitted.change_record.events.length).toBeLessThanOrEqual(admitted.change_record.options.max_events);
  expect(admitted.session.assertions.filter(row => row.active && row.source.startsWith('Bounded recording support'))).toHaveLength(2500);
  const supported = await ok(page, { kind: 'run', claim: 'dog(Adam).' });
  expect(supported.run.outcome.status).toBe('TRUE');
  await run(page, 'FALSE');
});
