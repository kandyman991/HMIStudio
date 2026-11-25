import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = fileURLToPath(new URL('../', import.meta.url));
const read = path => readFileSync(resolve(root, path), 'utf8').replaceAll('\r\n', '\n');
const json = path => JSON.parse(read(path));
const schema = json('roadmap/schema.json');

function requireThat(condition, message) {
  if (!condition) throw new Error(message);
}

// Only the JSON Schema keywords used by the checked-in schema are needed here.
export function validateShape(value, rule = schema, at = 'source') {
  const check = (condition, message) => requireThat(condition, at + ' ' + message);
  if (rule.$ref) {
    const def = schema.$defs[rule.$ref.slice('#/$defs/'.length)];
    return validateShape(value, def, at);
  }
  if (rule.oneOf) {
    const matches = rule.oneOf.filter(option => {
      try { validateShape(value, option, at); return true; } catch { return false; }
    });
    check(matches.length === 1, 'does not match exactly one schema');
  }
  if ('const' in rule) check(value === rule.const, 'has an invalid constant');
  if (rule.enum) check(rule.enum.includes(value), 'has an unknown value');
  switch (rule.type) {
    case 'object':
      check(value !== null && typeof value === 'object' && !Array.isArray(value), 'must be an object');
      for (const key of rule.required) check(Object.hasOwn(value, key), 'is missing ' + key);
      for (const [key, field] of Object.entries(value)) {
        check(Object.hasOwn(rule.properties, key), 'has unknown property ' + key);
        validateShape(field, rule.properties[key], at + '.' + key);
      }
      break;
    case 'array':
      check(Array.isArray(value) && value.length >= (rule.minItems ?? 0), 'must be an array with the required items');
      value.forEach((item, i) => validateShape(item, rule.items, at + '[' + i + ']'));
      break;
    case 'string':
      check(typeof value === 'string' && value.trim().length >= (rule.minLength ?? 0), 'must contain text');
      if (rule.pattern) check(new RegExp(rule.pattern).test(value), 'has an invalid format');
      break;
    case 'integer':
      check(Number.isSafeInteger(value) && value >= rule.minimum, 'must be a safe positive integer');
      break;
    case 'boolean':
      check(typeof value === 'boolean', 'must be a boolean');
      break;
  }
}

export const phases = {
  planned: 'Planned', in_progress: 'In progress', proposed: 'Proposed completion',
  merged: 'Merged', accepted: 'Accepted',
};
export const integrated = phase => phase === 'merged' || phase === 'accepted';
export const meets = (phase, gate) => phase === 'accepted' || (gate === 'merged' && phase === 'merged');
export const outputs = ['docs/MILESTONE_PLAN.md', 'roadmap/dashboard.html'];
const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
const linkRoot = repository => 'https://github.com/' + repository;
const integrationWorkflows = ['Desktop CI', 'Engineering handoff snapshot'];
const migrationPaths = ['docs/MILESTONE_PLAN.md', 'ROADMAP.md', 'docs/reuse/REUSE_MILESTONE_PLAN.md'];

function verifyDate(value) {
  const isDate = value.length === 10;
  const timestamp = new Date(isDate ? value + 'T00:00:00Z' : value);
  requireThat(Number.isFinite(timestamp.getTime()) && timestamp.toISOString().replace('.000Z', 'Z')
    .slice(0, value.length) === value, 'Invalid calendar date: ' + value);
}

function verifyDoc(path) {
  const file = path.split('#')[0];
  requireThat(file.split('/').every(part => part && part !== '.' && part !== '..'),
    'Invalid repository document path: ' + path);
  requireThat(statSync(resolve(root, file), { throwIfNoEntry: false })?.isFile(),
    'Missing repository document: ' + path);
}

function verifyBranch(branch) {
  requireThat(!branch.includes('..') && !branch.includes('//') && !branch.endsWith('.') &&
    branch.split('/').every(part => part && !part.startsWith('.') && !part.endsWith('.lock')),
  'Invalid branch: ' + branch);
}

export function validateRoadmap(context, tasks, options = {}) {
  validateShape(context, schema.$defs.context, 'context.json');
  requireThat(tasks.length > 0, 'No milestone files');
  const byId = new Map();
  const activeBranches = new Map();
  const verified = new Set();
  const verifyBase = sha => {
    if (!options.base || verified.has(sha)) return;
    try { git('merge-base', '--is-ancestor', sha, options.base); }
    catch { throw new Error(sha + ' is not a confirmed ancestor of the integration base'); }
    verified.add(sha);
  };
  const claimBranch = (branch, id) => {
    verifyBranch(branch);
    requireThat(!activeBranches.has(branch) || activeBranches.get(branch) === id,
      'Overlapping active branch ' + branch + ' for ' + activeBranches.get(branch) + ' and ' + id);
    activeBranches.set(branch, id);
  };
  for (const task of tasks) {
    const { id, phase, proposal, prs = [], assignment, acceptance, history = [], issues = [], docs = [] } = task;
    validateShape(task, schema.$defs.milestone, id);
    requireThat(!byId.has(id), 'Duplicate task ' + id);
    byId.set(id, task);

    const merged = prs.filter(pr => pr.state === 'merged');
    const active = prs.filter(pr => pr.state === 'open' || pr.state === 'draft');
    requireThat((phase === 'proposed') === Boolean(proposal), id + ' needs proposal text exactly in proposed phase');
    requireThat(phase !== 'proposed' || assignment || active.length, id + ' proposed completion needs an active PR or assignment');
    requireThat(phase !== 'in_progress' || !task.counts || assignment || active.length, id + ' active work needs a branch or assignment');
    requireThat(phase !== 'planned' || (!active.length && !assignment), id + ' planned task has active work');
    requireThat(!integrated(phase) || (merged.length && !active.length && !assignment),
      id + ' merged/accepted phase needs merged PR evidence and no active completion claim');
    const hasIntegrationChecks = merged.some(pr => integrationWorkflows.every(name =>
      pr.runs.some(run => run.name === name && run.state === 'completed' && run.result === 'success')));
    requireThat(!integrated(phase) || hasIntegrationChecks, id + ' needs successful exact-head Desktop CI and handoff evidence');
    requireThat((phase === 'accepted') === Boolean(acceptance), id + ' needs explicit milestone acceptance evidence exactly in accepted phase');
    requireThat(!acceptance || !task.blockers?.length, id + ' accepted task has an unresolved blocker');
    if (assignment) {
      claimBranch(assignment.branch, id);
      verifyDate(assignment.observedAt);
    }
    if (acceptance) {
      verifyBase(acceptance.commit);
      verifyDate(acceptance.date);
      verifyDoc(acceptance.record);
    }
    requireThat(new Set(prs.map(pr => pr.number)).size === prs.length, id + ' duplicate PR');
    requireThat(new Set(issues.map(issue => issue.number)).size === issues.length, id + ' duplicate issue');
    for (const pr of prs) {
      verifyDate(pr.observedAt);
      verifyBranch(pr.branch);
      if (active.includes(pr)) claimBranch(pr.branch, id);
      requireThat((pr.state === 'merged') === Boolean(pr.merge), id + ' inconsistent merge evidence on PR #' + pr.number);
      if (pr.merge) verifyBase(pr.merge);
      requireThat(new Set(pr.runs.map(run => run.name)).size === pr.runs.length &&
        new Set(pr.runs.map(run => run.id)).size === pr.runs.length, id + ' duplicate workflow evidence for PR #' + pr.number);
      for (const run of pr.runs) {
        requireThat(pr.head === run.head, id + ' workflow ' + run.id + ' is from a different PR head');
        requireThat((run.state === 'completed') !== (run.result === 'pending'), id + ' inconsistent workflow result for ' + run.id);
      }
    }
    for (const issue of issues) verifyDate(issue.observedAt);
    docs.forEach(verifyDoc);
    for (const entry of history) {
      verifyDate(entry.date);
      if (entry.record) verifyDoc(entry.record);
      if (entry.type === 'accepted') requireThat(acceptance, id + ' history asserts unrecorded acceptance');
      if (entry.type === 'merged') requireThat(merged.length, id + ' history asserts an unrecorded merge');
    }
  }

  const ranks = new Map();
  const stack = new Set();
  function visit(id) {
    requireThat(!stack.has(id), 'Dependency cycle: ' + [...stack, id].join(' → '));
    if (ranks.has(id)) return ranks.get(id);
    stack.add(id);
    let depth = 0;
    const dependencies = byId.get(id).dependencies;
    requireThat(new Set(dependencies.map(dep => dep.id)).size === dependencies.length, id + ' duplicate dependency');
    for (const dep of dependencies) {
      requireThat(byId.has(dep.id), id + ' depends on unknown milestone ' + dep.id);
      depth = Math.max(depth, visit(dep.id) + 1);
      if (byId.get(id).phase === 'accepted') {
        requireThat(meets(byId.get(dep.id).phase, dep.gate), id + ' accepted before prerequisite ' + dep.id);
      }
    }
    stack.delete(id);
    ranks.set(id, depth);
    return depth;
  }
  for (const id of byId.keys()) visit(id);

  requireThat(new Set(context.decisions.map(decision => decision.id)).size === context.decisions.length,
    'Duplicate decision ID');
  context.decisions.forEach(decision => verifyDoc(decision.record));
  requireThat(context.archives.map(archive => archive.path).join('\n') === migrationPaths.join('\n'),
    'Migration archives missing or out of order');
  if (options.verifyMigration) {
    verifyBase(context.migrationBase);
    for (const archive of context.archives) {
      requireThat(archive.text === git('show', context.migrationBase + ':' + archive.path).replaceAll('\r\n', '\n'),
        'Migration archive differs from source: ' + archive.path);
    }
  }

  return tasks.map(task => {
    const waiting = task.dependencies.filter(dep => !meets(byId.get(dep.id).phase, dep.gate));
    const eligible = task.counts && task.phase === 'planned' && waiting.length === 0 && !task.blockers?.length;
    return { ...task, rank: ranks.get(task.id), waiting, eligible };
  });
}

export function loadRoadmap(options = {}) {
  const context = json('roadmap/context.json');
  const directory = 'roadmap/milestones';
  const tasks = readdirSync(resolve(root, directory)).sort().map(file => {
    requireThat(/^[A-Z]+-\d{2}[A-Z]?(?:\.[0-9]+[A-Z]?)?\.json$/.test(file), 'Invalid milestone filename: ' + file);
    const task = json(directory + '/' + file);
    requireThat(file === task?.id + '.json', file + ' does not match its ID');
    return task;
  });
  return { context, tasks: validateRoadmap(context, tasks, options) };
}

const md = value => value.replace(/[\\|\x60<>\[\]*_]/g, '\\$&').replace(/\s*\n\s*/g, ' ');
const docHref = (path, prefix) => path.startsWith('docs/') && prefix === '../' ? path.slice(5) : prefix + path;
const docLink = (path, prefix) => '[' + md(path.split('#')[0]) + '](' + docHref(path, prefix) + ')';
const dependencyLabel = dependency => '[' + dependency.id + '](#' + dependency.id.toLowerCase() + ') (' + dependency.gate + ')';

function renderMarkdown(context, tasks) {
  const url = linkRoot(context.repository);
  const lines = [
    '<!-- Generated by node roadmap/generate.mjs from roadmap/; do not edit. -->',
    '# Detailed milestone plan', '',
    ...context.overview, '',
    'Open the [local dashboard](../roadmap/dashboard.html) from your checkout. See the [maintenance guide](../roadmap/README.md).', '',
    '## Current milestones', '', '| Milestone | Track | Phase | Dependencies |',
    '| --- | --- | --- | --- |',
    ...tasks.map(task => '| [' + task.id + '](#' + task.id.toLowerCase() + ') ' + md(task.title) +
      ' | ' + task.track + ' | ' + phases[task.phase] + ' | ' +
      (task.dependencies.map(dependencyLabel).join(', ') || '—') + ' |'),
  ];
  for (const track of ['Developer workflow', 'Engineering', 'Reuse']) {
    lines.push('', '## ' + track);
    for (const task of tasks.filter(item => item.track === track)) {
      const { id, title, phase, summary, criteria, blockers, prs, issues, assignment, proposal, acceptance, history, docs } = task;
      lines.push('', '<a id="' + id.toLowerCase() + '"></a>', '### ' + id + ' — ' + md(title), '',
        '**' + phases[phase] + '** · ' + md(summary), '', '**Acceptance criteria**', '',
        ...criteria.map(criterion => '- ' + md(criterion)));
      if (task.dependencies.length) {
        lines.push('', '**Dependencies**', '', ...task.dependencies.map(dep =>
          '- ' + dependencyLabel(dep) + (dep.note ? ' — ' + md(dep.note) : '')));
      }
      if (blockers) lines.push('', '**Blockers**', '', ...blockers.map(item => '- ' + md(item)));
      if (assignment) lines.push('', '**Assignment** · ' + md(assignment.owner) + ', ' +
        md(assignment.branch) + ' (recorded ' + assignment.observedAt + ')');
      if (proposal) lines.push('', '**Proposal** · ' + md(proposal));
      if (acceptance) lines.push('', '**Acceptance** · ' + md(acceptance.summary) + ' (' + acceptance.date + ', ' +
        docLink(acceptance.record, '../') + ', [commit](' + url + '/commit/' + acceptance.commit + '))');
      for (const pr of prs ?? []) {
        const runs = pr.runs.map(run => '[' + md(run.name) + ' #' + run.id + '](' + url + '/actions/runs/' +
          run.id + ') ' + run.result.replaceAll('_', ' ')).join(', ');
        lines.push('', '[PR #' + pr.number + '](' + url + '/pull/' + pr.number + ') · ' + pr.state +
          ' · head [' + pr.head.slice(0, 7) + '](' + url + '/commit/' + pr.head + ')' +
          (pr.merge ? ' · merged [' + pr.merge.slice(0, 7) + '](' + url + '/commit/' + pr.merge + ')' : '') +
          ' · snapshot ' + pr.observedAt + (runs ? ' · ' + runs : ' · CI unrecorded'));
      }
      for (const issue of issues ?? []) {
        lines.push('', '[Issue #' + issue.number + '](' + url + '/issues/' + issue.number + ') · ' +
          issue.state + ' · ' + md(issue.label) + ' · snapshot ' + issue.observedAt);
      }
      if (docs) lines.push('', 'Records · ' + docs.map(path => docLink(path, '../')).join(' · '));
      if (history) lines.push('', '**History**', '', ...history.map(event => '- ' + event.date + ' · ' +
        event.type.replaceAll('_', ' ') + ' · ' + md(event.summary) +
        (event.record ? ' · ' + docLink(event.record, '../') : '')));
    }
  }
  lines.push('', '## Engineering decisions', '',
    ...context.decisions.map(decision => '- **' + md(decision.id) + ' — ' + md(decision.title) + '** · ' +
      md(decision.summary) + ' · ' + docLink(decision.record, '../')));
  lines.push('', '## Historical roadmap records', '',
    'These snapshots preserve the three source plans at migration commit [' + context.migrationBase.slice(0, 7) +
    '](' + url + '/commit/' + context.migrationBase + '). Use the current milestone sections above for status.');
  for (const archive of context.archives) {
    const length = Math.max(3, ...Array.from(archive.text.matchAll(/~+/g), match => match[0].length + 1));
    const fence = '~'.repeat(length);
    lines.push('', '<details>', '<summary>' + md(archive.title) + '</summary>', '',
      fence + 'markdown', archive.text.trimEnd(), fence, '', '</details>');
  }
  return lines.join('\n') + '\n';
}

function renderDashboard(context, tasks) {
  const html = read('roadmap/dashboard.template');
  const js = read('roadmap/dashboard.js');
  const css = read('roadmap/dashboard.css');
  const hash = content => "'sha256-" + createHash('sha256').update(content).digest('base64') + "'";
  const policy = "default-src 'none'; script-src " + hash(js) + '; style-src ' + hash(css) +
    "; base-uri 'none'; form-action 'none'";
  const data = JSON.stringify({ context, tasks, phases }).replaceAll('<', '\\u003c');
  const replacements = { POLICY: policy, SCRIPT: js, STYLE: css, DATA: data };
  return html.replace(/%%(POLICY|SCRIPT|STYLE|DATA)%%/g, (_, key) => replacements[key]);
}

export function generate(options = {}) {
  const { context, tasks } = loadRoadmap(options);
  const rendered = [renderMarkdown(context, tasks), renderDashboard(context, tasks)];
  const outputRoot = options.outputRoot ?? root;
  const stale = [];
  outputs.forEach((path, i) => {
    if (!options.check) {
      writeFileSync(resolve(outputRoot, path), rendered[i], 'utf8');
      return;
    }
    try {
      if (readFileSync(resolve(outputRoot, path), 'utf8').replaceAll('\r\n', '\n') !== rendered[i]) {
        stale.push(path);
      }
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
      stale.push(path);
    }
  });
  requireThat(!stale.length, 'Regenerate with node roadmap/generate.mjs. Stale or missing: ' + stale.join(', '));
  return rendered;
}

if (resolve(process.argv[1] ?? '') === fileURLToPath(import.meta.url)) {
  try {
    const options = {};
    const args = process.argv.slice(2);
    while (args.length) {
      const flag = args.shift();
      if (flag === '--check') options.check = true;
      else if (flag === '--verify-migration') options.verifyMigration = true;
      else if (flag === '--base') {
        options.base = args.shift();
        requireThat(/^[a-f0-9]{40}$/.test(options.base ?? ''), '--base requires a full commit SHA');
      } else throw new Error('Unknown argument: ' + flag);
    }
    generate(options);
    console.log(options.check ? 'Roadmap data and generated files verified.' : 'Roadmap outputs regenerated.');
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
