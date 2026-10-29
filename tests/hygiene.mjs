// Repository hygiene: planning ids (tasks, requirements, observations) mean nothing outside the prototype's specs, the
// rulebook states behaviour (no word processor names, evidence or confidence wording, or fixtures), and CI's job plan
// (.moon/ci-plan.json) lists exactly the tasks moon runs in CI. Exits 1, with what it found, otherwise.
// Run: node tests/hygiene.mjs
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

const checks = [
  ['Planning ids', ['-nIE', String.raw`\bT[0-9]{3}\b|\b(FR|SC|OBS|WB)-[0-9]+\b|\(R[0-9]+\)|[;,] R[0-9]+\)`, '--', ':!tests/hygiene.mjs']],
  [
    'Rulebook wording',
    ['-nIiP', String.raw`\bword\b(?! ?(break|boundar|wrap))|microsoft|\boffice\b|measur|observ|inferr|confirm|evidence|research|experiment|harness|fixture|\[[MCI]\]`, '--', 'docs/docs/rulebook/*.md'],
  ],
];
let failed = false;
for (const [name, args] of checks) {
  // git grep exits 0 when it finds a line, 1 when it finds none.
  const found = spawnSync('git', ['grep', ...args], { stdio: 'inherit' }).status === 0;
  if (found) console.error(`${name} found (see above)`);
  failed ||= found;
}

// A task missing from the plan would never run in CI.
const query = spawnSync('pnpm', ['exec', 'moon', 'query', 'tasks'], { encoding: 'utf8', shell: process.platform === 'win32' });
const tasks = Object.values(JSON.parse(query.stdout).tasks).flatMap((byId) => Object.values(byId));
const inCi = tasks.filter((t) => t.options.runInCI !== false && t.options.runInCI !== 'skip' && !t.options.persistent).map((t) => t.target);
const planned = JSON.parse(readFileSync(new URL('../.moon/ci-plan.json', import.meta.url), 'utf8')).targets.jobs.flat();
const missing = inCi.filter((t) => !planned.includes(t));
const unknown = planned.filter((t) => !inCi.includes(t));
if (missing.length || unknown.length) {
  console.error(`CI plan: not in .moon/ci-plan.json: ${missing.join(', ') || 'none'}; not a CI task: ${unknown.join(', ') || 'none'}`);
  failed = true;
}
process.exit(failed ? 1 : 0);
