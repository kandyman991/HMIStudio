'use strict';

const $ = id => document.getElementById(id);
const { context, tasks, phases } = JSON.parse($('roadmap-data').textContent);
const github = 'https://github.com/' + context.repository;
const byId = new Map(tasks.map(task => [task.id, task]));
const taskNodes = new Map();
const filters = ['search', 'track', 'phase', 'owner', 'blocked'].map($);
const oneDay = 24 * 60 * 60 * 1000;
let selected = tasks.find(task => task.phase === 'proposed')?.id ?? tasks[0].id;
let visible = [];

function element(tag, text, className) {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (className) node.className = className;
  return node;
}

function link(label, url) {
  const node = element('a', label);
  node.href = url;
  return node;
}

function docLink(path, label = path.split('#')[0]) {
  return link(label, '../' + path);
}

function utc(value) {
  return value.replace('T', ' ').replace(/Z$/, ' UTC');
}

function time(value) {
  const node = element('time', utc(value));
  node.dateTime = value;
  return node;
}

function age(value) {
  const elapsed = Date.now() - Date.parse(value);
  if (elapsed < 0) return 'Local clock precedes observation';
  return elapsed >= oneDay ? 'Older than 24 hours' : 'Recorded within 24 hours';
}

function snapshot(value) {
  const node = element('p', undefined, age(value) === 'Older than 24 hours' ? 'warning' : '');
  node.append('Observed ', time(value), ' · ', age(value));
  return node;
}

function showProgress() {
  const counted = tasks.filter(task => task.counts);
  const total = counted.length;
  const merged = counted.filter(task => ['merged', 'accepted'].includes(task.phase)).length;
  const accepted = counted.filter(task => task.phase === 'accepted').length;
  const proposed = counted.filter(task => task.phase === 'proposed').length;
  function metric(value, label, showBar) {
    const node = element('div', undefined, 'metric');
    const ratio = total ? Math.round(100 * value / total) : 0;
    node.append(element('strong', showBar ? ratio + '%' : String(value)),
      element('span', value + ' of ' + total + ' · ' + label));
    if (showBar && total) {
      const bar = element('progress');
      bar.value = value;
      bar.max = total;
      bar.setAttribute('aria-label', label);
      node.append(bar);
    }
    return node;
  }
  $('progress').append(metric(merged, 'Engineering merged', true),
    metric(accepted, 'Milestone accepted', true), metric(proposed, 'Proposed completions', false));

  const observations = tasks.flatMap(task => [...(task.prs ?? []), ...(task.issues ?? [])]);
  const lastSeen = observations.map(item => item.observedAt).sort().at(-1);
  const indicator = $('snapshot-age');
  if (lastSeen) indicator.append('Latest observation ', time(lastSeen), ' · ', age(lastSeen));
  else indicator.textContent = 'No recorded GitHub observations';
}

function list(items) {
  const node = element('ul');
  for (const item of items) {
    const entry = element('li');
    entry.append(item);
    node.append(entry);
  }
  return node;
}

function detailSection(title, ...content) {
  const node = element('section');
  node.append(element('h4', title), ...content);
  return node;
}

function jumpButton(task, label) {
  const node = element('button', label, 'secondary');
  node.type = 'button';
  node.addEventListener('click', () => {
    if (!visible.some(item => item.id === task.id)) resetFilters();
    select(task.id, true);
  });
  return node;
}

function buildPrompt(task) {
  const lines = [
    'Resume HMI Studio (' + context.repository + ').',
    'Review ' + task.id + ' — ' + task.title + '.',
    '',
    'Recorded phase: ' + phases[task.phase] + '. ' + task.summary,
    task.assignment
      ? 'Coordinate with ' + task.assignment.owner + ' on ' + task.assignment.branch + ' before changing their work.'
      : 'Check current assignments and PRs before claiming work.',
    'Read AGENTS.md, CHAT_HANDOFF.md, the canonical milestone and relevant decisions. Verify current main, open PRs and applicable CI.',
    'Milestone record: roadmap/milestones/' + task.id + '.json',
    '',
    'Acceptance criteria:',
    ...task.criteria.map(value => '- ' + value),
  ];
  if (task.dependencies.length) {
    lines.push('', 'Prerequisites:', ...task.dependencies.map(dep => '- ' + dep.id + ' must be ' + dep.gate +
      (dep.note ? ' (' + dep.note + ')' : '')));
  }
  if (task.blockers?.length) lines.push('', 'Blockers:', ...task.blockers.map(value => '- ' + value));
  if (task.prs?.length) lines.push('', 'Recorded PRs:', ...task.prs.map(pr => '- ' + github + '/pull/' + pr.number));
  if (task.issues?.length) lines.push('', 'Recorded issues:', ...task.issues.map(issue => '- ' + github + '/issues/' + issue.number));
  if (task.docs?.length) lines.push('', 'Records:', ...task.docs.map(path => '- ' + path));
  lines.push('', 'Use a scoped branch and coordinate with active work. Update the canonical record at start, handoff and proposed completion.',
    'Regenerate both views, run targeted validation, and require independent review and exact-head hosted CI before any merge.',
    'Record a merge or acceptance only with verified evidence. Follow the designated milestone acceptance gates.');
  return lines.join('\n');
}

function promptDetails(task) {
  const details = element('details');
  const area = element('textarea');
  area.value = buildPrompt(task);
  area.readOnly = true;
  area.setAttribute('aria-label', 'Task prompt');
  const button = element('button', 'Copy prompt', 'primary');
  button.type = 'button';
  const message = element('p');
  message.setAttribute('role', 'status');
  button.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(area.value);
      message.textContent = 'Prompt copied.';
    } catch {
      area.focus();
      area.select();
      message.textContent = 'Prompt selected. Use your browser copy shortcut.';
    }
  });
  details.append(element('summary', 'Generate task prompt'), area, button, message);
  return details;
}

function renderDetail(task) {
  const detail = $('detail');
  const marker = element('span', phases[task.phase], 'phase-pill');
  marker.dataset.phase = task.phase;
  detail.replaceChildren(marker, element('h3', task.id + ' — ' + task.title), element('p', task.summary));
  if (task.proposal) detail.append(detailSection('Proposal', element('p', task.proposal)));
  if (task.acceptance) {
    const record = task.acceptance;
    detail.append(detailSection('Acceptance evidence', element('p', record.summary), time(record.date), ' · ',
      docLink(record.record), ' · ', link('Acceptance commit', github + '/commit/' + record.commit)));
  }
  detail.append(detailSection('Assignment', element('p', task.assignment
    ? task.assignment.owner + ' · ' + task.assignment.branch + ' · recorded ' + utc(task.assignment.observedAt)
    : 'No assignment recorded.'),
  ...(task.assignment && !task.prs?.some(pr => pr.branch === task.assignment.branch)
    ? [link('Find branch pull request', github + '/pulls?q=' +
      encodeURIComponent('is:pr head:' + task.assignment.branch))]
    : [])));
  if (task.dependencies.length) {
    detail.append(detailSection('Prerequisites', list(task.dependencies.map(dep => {
      const item = element('span');
      const waiting = task.waiting.some(missing => missing.id === dep.id);
      item.append(jumpButton(byId.get(dep.id), dep.id), ' · ' + (waiting ? 'Waiting for ' : 'Satisfied: ') + dep.gate +
        (dep.note ? ' · ' + dep.note : ''));
      return item;
    }))));
  }
  if (task.blockers) detail.append(detailSection('Blockers', list(task.blockers)));
  detail.append(detailSection('Acceptance criteria', list(task.criteria)));
  if (task.prs?.length || task.issues?.length) {
    const evidence = detailSection('GitHub evidence');
    for (const pr of task.prs ?? []) {
      const entry = element('section');
      const meta = element('p');
      meta.append(link('PR #' + pr.number, github + '/pull/' + pr.number), ' · ' + pr.state + ' · ',
        link('Head ' + pr.head.slice(0, 7), github + '/commit/' + pr.head));
      if (pr.merge) meta.append(' · ', link('Merge ' + pr.merge.slice(0, 7), github + '/commit/' + pr.merge));
      entry.append(meta, element('p', pr.branch), snapshot(pr.observedAt));
      if (pr.runs.length) {
        entry.append(list(pr.runs.map(run => {
          const status = run.state === 'completed' ? run.result : run.state;
          const item = element('span');
          item.append(link(run.name, github + '/actions/runs/' + run.id), ' · ' + status.replaceAll('_', ' '));
          return item;
        })));
      } else entry.append(element('p', 'Exact-head workflow results unrecorded.'));
      evidence.append(entry);
    }
    for (const issue of task.issues ?? []) {
      const entry = element('section');
      entry.append(link('Issue #' + issue.number + ' · ' + issue.label, github + '/issues/' + issue.number),
        ' · ' + issue.state, snapshot(issue.observedAt));
      evidence.append(entry);
    }
    detail.append(evidence);
  }
  if (task.docs) detail.append(detailSection('Records', list(task.docs.map(path => docLink(path)))));
  if (task.history) detail.append(detailSection('Milestone history', list(task.history.map(event => {
    const item = element('span');
    item.append(time(event.date), ' · ' + event.summary);
    if (event.record) item.append(' · ', docLink(event.record));
    return item;
  }))));
  detail.append(promptDetails(task));
  detail.scrollTop = 0;
}

function drawEdges() {
  const canvas = $('edges');
  const inner = $('graph-inner');
  const bounds = inner.getBoundingClientRect();
  const pixelRatio = Math.min(window.devicePixelRatio || 1, 2);
  canvas.width = Math.ceil(bounds.width * pixelRatio);
  canvas.height = Math.ceil(bounds.height * pixelRatio);
  canvas.style.width = bounds.width + 'px';
  canvas.style.height = bounds.height + 'px';
  const pen = canvas.getContext('2d');
  if (!pen) return;
  pen.scale(pixelRatio, pixelRatio);
  for (const task of visible) {
    const to = taskNodes.get(task.id).getBoundingClientRect();
    for (const dep of task.dependencies) {
      const from = taskNodes.get(dep.id)?.getBoundingClientRect();
      if (!from) continue;
      const x1 = from.right - bounds.left;
      const x2 = to.left - bounds.left;
      const y1 = (from.top + from.bottom) / 2 - bounds.top;
      const y2 = (to.top + to.bottom) / 2 - bounds.top;
      const bend = (x1 + x2) / 2;
      const active = task.id === selected || dep.id === selected;
      const waiting = task.waiting.some(missing => missing.id === dep.id);
      pen.lineWidth = active ? 2 : 1;
      pen.strokeStyle = active ? '#91e8d1' : '#5a7283';
      pen.setLineDash(waiting ? [4, 4] : []);
      pen.beginPath();
      pen.moveTo(x1, y1);
      pen.bezierCurveTo(bend, y1, bend, y2, x2 - 2, y2);
      pen.stroke();
      pen.setLineDash([]);
      pen.beginPath();
      pen.moveTo(x2 - 8, y2 - 4);
      pen.lineTo(x2 - 2, y2);
      pen.lineTo(x2 - 8, y2 + 4);
      pen.stroke();
    }
  }
}

function select(id, focusDetail = false) {
  selected = id;
  const task = byId.get(id);
  for (const [nodeId, node] of taskNodes) {
    node.setAttribute('aria-pressed', String(nodeId === id));
    node.classList.toggle('connected', task.dependencies.some(dep => dep.id === nodeId) ||
      byId.get(nodeId).dependencies.some(dep => dep.id === id));
  }
  renderDetail(task);
  drawEdges();
  if (focusDetail) $('detail').focus();
}

function populateGraph() {
  const [search, track, phase, owner, blocked] = filters;
  const query = search.value.trim().toLowerCase();
  visible = tasks.filter(task => {
    const hasBlocker = Boolean(task.blockers?.length || task.waiting.length);
    return (!track.value || task.track === track.value) &&
      (!phase.value || task.phase === phase.value) &&
      (!owner.value || Boolean(task.assignment) === (owner.value === 'assigned')) &&
      (!blocked.checked || hasBlocker) &&
      [task.id, task.title, task.summary, task.assignment?.owner, task.assignment?.branch,
        ...(task.prs ?? []).map(pr => pr.branch)].join(' ').toLowerCase().includes(query);
  });
  const columns = $('columns');
  columns.replaceChildren();
  taskNodes.clear();
  $('matches').textContent = visible.length + ' of ' + tasks.length + ' milestones';
  if (!visible.length) {
    columns.append(element('p', 'No milestones match these filters.'));
    $('detail').replaceChildren(element('p', 'Choose different filters to select a milestone.'));
    drawEdges();
    return;
  }
  const byRank = new Map();
  for (const task of visible) {
    if (!byRank.has(task.rank)) byRank.set(task.rank, []);
    byRank.get(task.rank).push(task);
  }
  for (const rank of [...byRank.keys()].sort((a, b) => a - b)) {
    const column = element('div', undefined, 'graph-column');
    column.append(element('h3', rank ? 'Dependency depth ' + rank : 'No prerequisites', 'column-title'));
    for (const task of byRank.get(rank)) {
      const node = element('button', undefined, 'node');
      node.type = 'button';
      node.dataset.phase = task.phase;
      node.setAttribute('aria-controls', 'detail');
      node.append(element('span', task.id, 'node-id'), element('span', task.title, 'node-title'),
        element('span', phases[task.phase], 'node-phase'));
      node.addEventListener('click', () => select(task.id, true));
      taskNodes.set(task.id, node);
      column.append(node);
    }
    columns.append(column);
  }
  select(taskNodes.has(selected) ? selected : visible[0].id);
}

function resetFilters() {
  for (const filter of filters) {
    if (filter.type === 'checkbox') filter.checked = false;
    else filter.value = '';
  }
  populateGraph();
}

function showEligible() {
  const area = $('eligible');
  const next = tasks.filter(task => task.eligible);
  if (next.length) area.append(list(next.map(task => jumpButton(task, task.id + ' · ' + task.title))));
  else area.append(element('p', 'No planned task currently meets every prerequisite.'));
  const waitingForAcceptance = new Set(tasks.flatMap(task => task.waiting.filter(dep => dep.gate === 'accepted').map(dep => dep.id)));
  const followUps = tasks.filter(task => task.phase === 'merged' && waitingForAcceptance.has(task.id)
    && task.issues?.some(issue => issue.state === 'open'));
  if (followUps.length) {
    area.append(element('h4', 'Acceptance follow-up'), list(followUps.map(task =>
      jumpButton(task, task.id + ' · ' + task.title))));
  }
  const proposals = tasks.filter(task => task.phase === 'proposed');
  if (proposals.length) {
    area.append(element('h4', 'Completion proposals'), list(proposals.map(task =>
      jumpButton(task, task.id + ' · ' + task.title))));
  }
}

function showRecords() {
  const decisions = $('decisions');
  for (const record of context.decisions) {
    const details = element('details');
    details.append(element('summary', record.id + ' · ' + record.title),
      element('p', record.summary), docLink(record.record, 'Decision record'));
    decisions.append(details);
  }
  const history = $('history');
  const changes = tasks.flatMap(task => (task.history ?? []).map(event => ({ ...event, id: task.id })));
  changes.sort((a, b) => a.date === b.date ? a.id.localeCompare(b.id) : a.date < b.date ? 1 : -1);
  history.append(list(changes.map(event => {
    const entry = element('span');
    entry.append(time(event.date), ' · ' + event.id + ' · ' + event.summary);
    return entry;
  })), docLink('CHAT_HANDOFF.md', 'Curated engineering handoff'));
  for (const record of context.archives) {
    const details = element('details');
    details.append(element('summary', record.title), element('pre', record.text));
    history.append(details);
  }
}

for (const track of new Set(tasks.map(task => task.track))) $('track').append(new Option(track, track));
for (const [phase, label] of Object.entries(phases)) $('phase').append(new Option(label, phase));
for (const filter of filters) filter.addEventListener(filter.type === 'search' ? 'input' : 'change', populateGraph);
$('reset').addEventListener('click', resetFilters);
showProgress();
populateGraph();
showEligible();
showRecords();
new ResizeObserver(drawEdges).observe($('graph-inner'));
