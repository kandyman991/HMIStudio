import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { generate, validateRoadmap } from './generate.mjs';

const read = path => JSON.parse(readFileSync(fileURLToPath(new URL(path, import.meta.url)), 'utf8'));
const context = read('./context.json');
const milestone = id => read('./milestones/' + id + '.json');
const task = (id, dependencies = []) => ({
  id, title: id, summary: 'A regression fixture', track: 'Engineering', counts: true,
  phase: 'planned', criteria: ['Demonstrate the expected behavior'], dependencies,
});
const dep = (id, gate = 'merged') => ({ id, gate });
const validate = tasks => validateRoadmap(context, tasks);

test('generates the same bytes twice and preserves exact original sources', () => {
  const one = generate({ check: true, verifyMigration: true });
  const two = generate({ check: true, verifyMigration: true });
  assert.deepEqual(one.map(text => Buffer.from(text)), two.map(text => Buffer.from(text)));
  assert.ok(one.every(text => text.includes('do not edit')));
  assert.ok(one[1].includes("default-src 'none'"));
});

test('detects missing or manually edited outputs without modifying the checkout', () => {
  const outputRoot = mkdtempSync(join(tmpdir(), 'roadmap-test-'));
  try {
    mkdirSync(join(outputRoot, 'docs'));
    mkdirSync(join(outputRoot, 'roadmap'));
    assert.throws(() => generate({ outputRoot, check: true }), /Stale or missing: docs\/MILESTONE_PLAN\.md, roadmap\/dashboard\.html/);
    generate({ outputRoot });
    generate({ outputRoot, check: true });
    writeFileSync(join(outputRoot, 'roadmap', 'dashboard.html'), '<p>Outdated</p>');
    assert.throws(() => generate({ outputRoot, check: true }), /Stale or missing: roadmap\/dashboard\.html/);
  } finally {
    rmSync(outputRoot, { recursive: true });
  }
});

test('rejects unknown properties and invalid field shapes', () => {
  assert.throws(() => validate([{ ...task('FIX-01'), status: 'done' }]), /unknown property status/);
  assert.throws(() => validate([{ ...task('FIX-01'), phase: 'done' }]), /unknown value/);
  assert.throws(() => validate([{ ...task('FIX-01'), criteria: [] }]), /required items/);
  assert.throws(() => validate([{ ...task('FIX-01'), docs: ['docs/../AGENTS.md'] }]), /Invalid repository document path/);
});

test('rejects duplicate IDs, unknown prerequisites and cycles', () => {
  assert.throws(() => validate([task('FIX-01'), task('FIX-01')]), /Duplicate task/);
  assert.throws(() => validate([task('FIX-01', [dep('FIX-02')])]), /unknown milestone FIX-02/);
  assert.throws(() => validate([task('FIX-01', [dep('FIX-01')])]), /Dependency cycle/);
  assert.throws(() => validate([task('FIX-01', [dep('FIX-02')]), task('FIX-02', [dep('FIX-01')])]),
    /FIX-01 → FIX-02 → FIX-01/);
  assert.throws(() => validate([task('FIX-01'), task('FIX-02', [dep('FIX-01'), dep('FIX-01', 'accepted')])]),
    /duplicate dependency/);
});

test('separates merged and accepted prerequisites in next task eligibility', () => {
  const foundation = milestone('RUST-01');
  const tasks = [foundation, task('FIX-01', [dep(foundation.id)]),
    task('FIX-02', [dep(foundation.id, 'accepted')]),
    { ...task('FIX-03'), blockers: ['Owner decision'] },
    { ...task('FIX-04'), counts: false }];
  const [, ready, waiting, blocked, summary] = validate(tasks);
  assert.equal(ready.eligible, true);
  assert.equal(ready.rank, 1);
  assert.equal(waiting.eligible, false);
  assert.deepEqual(waiting.waiting, [dep(foundation.id, 'accepted')]);
  assert.equal(blocked.eligible, false);
  assert.equal(summary.eligible, false);
});

test('keeps a green active proposal distinct from a merge and from acceptance', () => {
  const candidate = milestone('REUSE-02C.4A');
  candidate.dependencies = [];
  assert.equal(validate([candidate])[0].eligible, false);
  assert.throws(() => validate([{ ...candidate, phase: 'merged', proposal: undefined }]), /must contain text/);
  delete candidate.proposal;
  assert.throws(() => validate([{ ...candidate, phase: 'merged' }]), /needs merged PR evidence/);
  const foundation = milestone('RUST-01');
  assert.throws(() => validate([{ ...foundation, phase: 'accepted' }]), /needs explicit milestone acceptance/);
  const failed = structuredClone(foundation);
  failed.prs[0].runs[0].result = 'failure';
  assert.throws(() => validate([failed]), /needs successful exact-head Desktop CI and handoff evidence/);
  assert.throws(() => validate([{ ...foundation, phase: 'accepted', acceptance: {
    commit: context.migrationBase, date: '2026-11-25', summary: 'fixture', record: 'docs/DOES_NOT_EXIST.md',
  } }]), /Missing repository document/);
});

test('rejects impossible times, unrelated merge commits and malformed exact-head CI', () => {
  const foundation = milestone('RUST-01');
  const pr = foundation.prs[0];
  pr.runs[0].head = 'a'.repeat(40);
  assert.throws(() => validate([foundation]), /different PR head/);
  pr.runs[0].head = pr.head;
  pr.runs[0].result = 'pending';
  assert.throws(() => validate([foundation]), /inconsistent workflow result|needs successful exact-head/);
  pr.runs[0].result = 'success';
  pr.observedAt = '2026-02-30T11:00:00Z';
  assert.throws(() => validate([foundation]), /Invalid calendar date/);
  pr.observedAt = '2026-11-25T11:00:00Z';
  pr.merge = 'a'.repeat(40);
  assert.throws(() => validateRoadmap(context, [foundation], { base: context.migrationBase }), /not a confirmed ancestor/);
});

test('flags overlapping branch claims across active milestones', () => {
  const assignment = { owner: 'Fixture', branch: 'feat/fixture', observedAt: '2026-11-25T11:00:00Z' };
  const active = id => ({ ...task(id), phase: 'in_progress', assignment });
  assert.throws(() => validate([active('FIX-01'), active('FIX-02')]), /Overlapping active branch/);
});
