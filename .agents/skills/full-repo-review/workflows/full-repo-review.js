export const meta = {
  name: 'full-repo-review',
  description: 'Full-repo multi-agent code review of Wardnet; drafts session-sized GitHub issues. Drafts only — nothing is filed.',
  whenToUse:
    'A whole-codebase audit whose output is a batch of issues to fix in later sessions. Run docs-accuracy FIRST — this builds its conventions digest from those docs. Expensive (~200 agents); explicit opt-in only.',
  phases: [
    { title: 'Digest', detail: 'distil the house rules from the convention docs' },
    { title: 'Sweep', detail: 'repo-wide invariant checks a diff review cannot do' },
    { title: 'Review', detail: '4-axis panel per unit (correctness, security, design, conventions)' },
    { title: 'Verify', detail: 'adversarial skeptics try to refute every finding' },
    { title: 'Cluster', detail: 'dedup and group survivors into session-sized themes' },
    { title: 'Draft', detail: 'one issue body per theme' },
    { title: 'Critic', detail: 'what did this review miss?' },
    { title: 'Write', detail: 'scribe writes report.md, issues/, manifest.json' },
  ],
}

// ---------------------------------------------------------------------------
// Config. args = { date, target, mode: 'full'|'smoke', units?: [key], sha? }
// ---------------------------------------------------------------------------

// The host may hand `args` through as a JSON string rather than an object.
const A = typeof args === 'string' ? JSON.parse(args) : args || {}

// SKILL.md always passes an absolute `target` (the worktree to review). The
// relative fallback only exists so the script is runnable ad hoc from the repo root.
const TARGET = A.target || 'main'
// Conventions may live in a different worktree than the code under review (e.g.
// when the docs-accuracy fixes are not merged yet). Code from TARGET, rules from DOCS_FROM.
const DOCS_FROM = A.docsFrom || TARGET
const DATE = A.date || 'undated'
const SHA = A.sha || 'unknown'
const MODE = A.mode || 'full'
const SMOKE = MODE === 'smoke'
// A focused gap-fill run re-does specific units without redoing the repo-wide
// sweeps or the docs-audit seeds (both already covered by the main pass).
const SKIP_SWEEPS = A.skipSweeps === true
const SKIP_SEEDS = A.skipSeeds === true
// Extra standing instruction injected into every reviewer prompt this run.
const EXTRA = A.extra || ''
const OUT = `${TARGET}/.reviews/${DATE}`

// Paths never worth a reviewer's tokens. Build output that is checked into git,
// vendored JS, two ~39k-row OUI csvs, lockfiles, generated clients, snapshots.
const EXCLUDES = [
  '**/dist/**',
  '**/node_modules/**',
  '**/target/**',
  '**/coverage/**',
  '**/__snapshots__/**',
  '**/*.d.ts',
  'source/admin-site/web/dist/**',
  'source/daemon/crates/wardnetd-api/assets/vendor/**',
  '**/data/oui.csv',
  '**/Cargo.lock',
  '**/yarn.lock',
  'docs/openapi.json',
  'source/marketing-site/src/generated/**',
  'source/marketing-site/public/{releases,api-specs,api-docs,sdk-docs}/**',
]

// ---------------------------------------------------------------------------
// Review units. Sized by PRODUCTION loc — roughly half the daemon's 136k lines
// are in-crate test modules, so raw loc would over-provision badly.
// `component` maps onto the repo's existing GitHub label taxonomy.
// ---------------------------------------------------------------------------

const S = 'source/daemon/crates/wardnetd-services/src'

const UNITS = [
  // ---- daemon: production ------------------------------------------------
  {
    key: 'daemon-domain',
    stack: 'rust',
    loc: 5500,
    paths: ['source/daemon/crates/wardnet-common/src'],
    component: ['component:daemon'],
    role: 'Shared domain types, API DTOs, config, and the event bus. Everything else depends on this, so a bad type here is a bug everywhere.',
  },
  {
    key: 'daemon-data',
    stack: 'rust',
    loc: 7800,
    paths: ['source/daemon/crates/wardnetd-data/src', 'source/daemon/crates/wardnetd-data/migrations'],
    component: ['component:daemon'],
    role: 'Repository traits + SQLite impls, migrations, secret store (age), OUI lookup. SQL correctness, transaction boundaries, and secret handling live here.',
  },
  {
    key: 'daemon-svc-dns-net',
    stack: 'rust',
    loc: 10000,
    paths: [`${S}/dns`, `${S}/dns_filter`, `${S}/dns_local`, `${S}/dhcp`, `${S}/ddns`, `${S}/tls`],
    component: ['component:daemon', 'component:dns', 'component:dhcp'],
    role: 'DNS resolution + filtering + local authoritative view, DHCP leases, DDNS, ACME/TLS. This is the code that touches untrusted packets from the LAN.',
    context: 'Read the Local-DNS and DDNS subsystem sections of .agents/architecture.md first — resolution pipeline order and the ArcSwap AuthoritativeView are load-bearing.',
  },
  {
    key: 'daemon-svc-tunnel-routing',
    stack: 'rust',
    loc: 8000,
    paths: [`${S}/tunnel`, `${S}/routing`, `${S}/vpn`, `${S}/network_zone`, `${S}/zone_enforcement`, `${S}/zone_exception`, `${S}/rule_request`, `${S}/stats`, `${S}/subnet`, `${S}/garp`],
    component: ['component:daemon', 'component:tunnel', 'component:nftables'],
    role: 'WireGuard tunnels, policy routing, network zones and their enforcement. A bug here means traffic leaks outside the tunnel a user believes they are on — treat leaks as severe.',
    context: 'Read the Network-Zone enforcement section of .agents/architecture.md. The egress gate and admin-UI gate are separate; same-subnet peer traffic is a KNOWN, documented limit — do not report it as a novel finding.',
  },
  {
    key: 'daemon-svc-platform',
    stack: 'rust',
    loc: 9500,
    paths: [`${S}/auth`, `${S}/backup`, `${S}/update`, `${S}/system`, `${S}/logging`, `${S}/push`, `${S}/cloud`, `${S}/device`, `${S}/health`, `${S}/jobs`, `${S}/entitlement`, `${S}/event`, `${S}/command`, `${S}/lib.rs`],
    component: ['component:daemon', 'component:devices'],
    role: 'Auth (argon2 sessions), backup/restore, updates, health + watchdog, push (VAPID), device discovery, entitlements. Auth and backup are the two highest-stakes modules in the repo.',
    context: 'Read .agents/auth.md and .agents/backup.md. The watchdog invariant (the hardware pet is NEVER health-gated) is in architecture.md and is deliberate.',
  },
  {
    key: 'daemon-api',
    stack: 'rust',
    loc: 7200,
    paths: ['source/daemon/crates/wardnetd-api/src'],
    component: ['component:api', 'component:daemon'],
    role: 'Axum HTTP layer: handlers, middleware, OpenAPI annotations, websockets. Handlers must be thin — logic belongs in services.',
  },
  {
    key: 'daemon-infra-linux',
    stack: 'rust',
    loc: 10900,
    paths: ['source/daemon/crates/wardnetd/src'],
    component: ['component:daemon', 'component:nftables'],
    role: 'The Linux binary: nftables, netlink, WireGuard, the DNS and DHCP servers, TLS server, background runners, main.rs. Runs with elevated privilege and parses hostile input.',
  },
  {
    key: 'daemon-tooling',
    stack: 'rust',
    loc: 6300,
    paths: [
      'source/daemon/crates/wardnetd-mock/src',
      'source/daemon/crates/wardnet-test-agent/src',
      'source/daemon/crates/wardnet-postupgrade/src',
      'source/daemon/crates/wardnet-postupgrade-runner/src',
    ],
    component: ['component:daemon'],
    role: 'Dev/mock binary, Pi-side test agent, post-upgrade verify+rollback. Lower stakes than the daemon — EXCEPT post-upgrade, where a bug bricks a user\'s box on update. Weight accordingly.',
  },

  // ---- frontend ----------------------------------------------------------
  {
    key: 'admin-site-pages',
    stack: 'react',
    loc: 6400,
    paths: ['source/admin-site/src/pages', 'source/admin-site/src/hooks', 'source/admin-site/src/lib', 'source/admin-site/src/services', 'source/admin-site/src/stores', 'source/admin-site/src/App.tsx', 'source/admin-site/src/main.tsx'],
    component: ['component:web-ui'],
    role: 'Admin site routing, pages, data fetching, Zustand stores. Pages are the ONLY layer allowed to call the API.',
  },
  {
    key: 'admin-site-features',
    stack: 'react',
    loc: 7600,
    paths: ['source/admin-site/src/components/features'],
    component: ['component:web-ui'],
    role: 'Feature components — tables, charts, forms. Must not call the API directly.',
  },
  {
    key: 'admin-site-ui',
    stack: 'react',
    loc: 4700,
    paths: ['source/admin-site/src/components/core', 'source/admin-site/src/components/compound', 'source/admin-site/src/components/layouts'],
    component: ['component:web-ui'],
    role: 'Presentational layers: core/ui primitives, compound components, layouts. Strict layering — these may not reach upward.',
  },
  {
    key: 'web-shared-lib',
    stack: 'react',
    loc: 5400,
    paths: ['source/web/src'],
    component: ['component:web-ui'],
    role: '@wardnet/web — shared TanStack Query hooks, stores, utils consumed by all three front-ends. A bug here multiplies across every app.',
  },
  {
    key: 'sdk',
    stack: 'typescript',
    loc: 4600,
    paths: ['source/sdk/wardnet-js/src'],
    component: ['component:sdk'],
    role: '@wardnet/js — the public, zero-runtime-dep TypeScript client. This is a PUBLISHED PUBLIC API: breaking changes and sloppy types have blast radius beyond this repo. Error handling and type fidelity to the OpenAPI contract matter most.',
  },
  {
    key: 'admin-app-pwa',
    stack: 'react',
    loc: 5800,
    paths: ['source/admin-app/src'],
    component: ['component:web-ui'],
    role: 'Admin mobile PWA (vite-plugin-pwa, workbox, custom sw.ts). Service-worker caching bugs are the classic failure mode: stale shells, unbounded caches, auth state cached across users.',
  },
  {
    key: 'user-app-pwa',
    stack: 'react',
    loc: 4100,
    paths: ['source/user-app/src'],
    component: ['component:web-ui'],
    role: 'End-user PWA — device-keyed identity (not admin sessions), Leaflet maps, IndexedDB. Identity confusion here is a security finding, not a design nit. See CONTEXT.md on device-keyed vs admin-session identity.',
  },
  {
    key: 'marketing-site',
    stack: 'react',
    loc: 4000,
    paths: ['source/marketing-site/src'],
    component: ['component:site'],
    role: 'Public marketing site. Lowest stakes in the repo: no auth, no user data. Hold findings to a HIGH bar — do not pad the review with cosmetic nits from here.',
  },

  // ---- test quality ------------------------------------------------------
  {
    key: 'tests-daemon-services',
    stack: 'rust',
    lens: 'test-quality',
    loc: 32900,
    paths: [`${S}/**/tests`],
    component: ['component:daemon'],
    role: 'Service-layer test suites (hand-written mocks, no mocking library).',
  },
  {
    key: 'tests-daemon-api',
    stack: 'rust',
    lens: 'test-quality',
    loc: 15000,
    paths: ['source/daemon/crates/wardnetd-api/src/api/tests'],
    component: ['component:api'],
    role: 'API handler test suites.',
  },
  {
    key: 'tests-daemon-core',
    stack: 'rust',
    lens: 'test-quality',
    loc: 17000,
    paths: ['source/daemon/crates/wardnetd/src/**/tests', 'source/daemon/crates/wardnetd-data/src/**/tests', 'source/daemon/crates/wardnet-common/src/**/tests'],
    component: ['component:daemon'],
    role: 'Infrastructure, repository (in-memory SQLite), and domain test suites.',
  },
  {
    key: 'tests-admin-site',
    stack: 'react',
    lens: 'test-quality',
    loc: 14100,
    paths: ['source/admin-site/tests'],
    component: ['component:web-ui'],
    role: 'Admin-site Vitest + RTL suites.',
  },
  {
    key: 'tests-web-sdk',
    stack: 'react',
    lens: 'test-quality',
    loc: 4800,
    paths: ['source/web/src/**/*.test.tsx', 'source/web/src/**/*.test.ts', 'source/sdk/wardnet-js/tests'],
    component: ['component:web-ui', 'component:sdk'],
    role: 'Shared-lib and SDK test suites.',
  },
  {
    key: 'tests-e2e',
    stack: 'playwright',
    lens: 'test-quality',
    loc: 7600,
    paths: ['source/end2end-tests/web-ui', 'source/end2end-tests/daemon'],
    component: ['component:ci'],
    role: 'Playwright (web-ui) + Vitest (daemon) end-to-end suites. data-testid is the mandated locator strategy (ADR 0015).',
  },
]

// Findings handed over by the docs-accuracy run (2026-07-11): rules the docs
// still mean and the code has broken. They enter the pipeline as ordinary
// findings and are adversarially verified like everything else — a seed that
// does not hold up dies in the Verify phase, same as any reviewer's claim.
const SEEDS = [
  {
    severity: 'RED',
    category: 'security:missing-auth-guard',
    file: 'source/daemon/crates/wardnetd-services/src/logging/service.rs',
    line: 'LogServiceImpl::list_log_files, ::download_log_file',
    issue: 'LogServiceImpl has no auth_context guard on any method; every other service file has at least one.',
    evidence: 'No auth_context::require_admin()? / require_authenticated()? anywhere in logging/service.rs. Reachable from wardnetd-api/src/api/system.rs:79 (behind AdminAuth middleware).',
    failure_scenario: 'Defense-in-depth gap: the service trusts its caller. Any future caller that is not behind AdminAuth — a background runner, a new handler, a test harness — reaches log listing and log download with no identity check. Log files can contain sensitive network data.',
    proposed_action: 'Add require_admin()? as the first statement of both methods, per .agents/auth.md rule 1.',
    confidence: 'high',
    unit: 'daemon-svc-platform',
    axis: 'security',
    component: ['component:daemon'],
    id: 'seed-logservice-guard',
  },
  {
    severity: 'RED',
    category: 'security:missing-auth-context',
    file: 'source/daemon/crates/wardnetd-services/src/stats/flush_runner.rs',
    line: '153,169',
    issue: 'StatsFlushRunner calls StatsService::run_flush/run_maintenance with no with_context wrapper, and the callees carry no guard — the path has no auth check at all.',
    evidence: 'flush_runner.rs:153,169 call the service directly. stats/service.rs:101-120 has no require_* guard. Contrast tunnel_idle.rs:143-254, which wraps its calls in auth_context::with_context(AuthContext::Admin { admin_id: Uuid::nil() }, ...).',
    failure_scenario: 'Background task runs outside HTTP middleware, so no AuthContext is set. Neither the caller nor the callee establishes or checks one, so the auth model has a hole on this path — and if a guard is ever added to StatsService, the runner breaks at runtime instead of failing at compile time.',
    proposed_action: 'Wrap both calls in auth_context::with_context(AuthContext::Admin { admin_id: Uuid::nil() }, ...) per .agents/auth.md rule 3, and add the guard to the StatsService methods per rule 1.',
    confidence: 'high',
    unit: 'daemon-svc-platform',
    axis: 'security',
    component: ['component:daemon'],
    id: 'seed-statsflush-context',
  },
  {
    severity: 'AMBER',
    category: 'conventions:sql-const',
    file: 'source/daemon/crates/wardnetd-data/src/repository/sqlite/device.rs',
    line: '84,93,106,115,194 (+ network_zone.rs:83,91,100,109; zone_exception.rs:104,112)',
    issue: 'Eleven sites format!() a const column list into an otherwise-fixed SELECT instead of hoisting the whole statement to a const.',
    evidence: 'e.g. `let query = format!("SELECT {SELECT_COLS} FROM devices WHERE last_ip = ?");` — SELECT_COLS is itself a const, so the entire query is constant.',
    failure_scenario: 'Not an injection risk (every component is constant), but it pays a heap allocation per call on repository hot paths, and .agents/code-conventions.md requires const query strings for fixed SQL.',
    proposed_action: 'Hoist each to a `const` query string.',
    confidence: 'high',
    unit: 'daemon-data',
    axis: 'conventions',
    component: ['component:daemon'],
    id: 'seed-sql-const',
  },
  {
    severity: 'GREEN',
    category: 'conventions:test-layout',
    file: 'source/daemon/crates/wardnetd-services/src/entitlement.rs',
    line: 'and ~13 other files',
    issue: '~14 daemon source files carry an inline #[cfg(test)] mod tests block, which .agents/testing.md forbids without exception.',
    evidence: 'Inline test modules in wardnetd-services/src/{entitlement.rs, subnet.rs, update/service.rs, tunnel/service.rs, push/sender.rs, dns/service.rs, dns/log_sink.rs}; wardnetd/src/{garp_pnet.rs, tunnel_exit_probe.rs, system/pnet_network_probe.rs, system/proc_net_inspector.rs, dns/rate_limit.rs}; wardnetd-mock/src/backends/noop_exit_probe.rs; wardnet-test-agent/src/client/ping.rs.',
    failure_scenario: 'Convention violation. .agents/testing.md: "Tests must live in separate files. Never put #[test] or #[tokio::test] blocks inline in source files." The rule was deliberately kept strict; these files are debt.',
    proposed_action: 'Move each inline test module to src/<layer>/tests/<module>.rs with a #[cfg(test)] mod tests; declaration in the layer mod.rs.',
    confidence: 'high',
    unit: 'daemon-svc-platform',
    axis: 'conventions',
    component: ['component:daemon'],
    id: 'seed-inline-tests',
  },
  {
    severity: 'GREEN',
    category: 'design:dead-code',
    file: 'source/daemon/crates/wardnetd/build.rs',
    line: 'generate_oui_data()',
    issue: 'wardnetd/build.rs still generates OUI data (and ships its own ~39k-row oui.csv copy), but nothing in wardnetd/src includes the generated oui_data.rs — the generation moved to wardnetd-data.',
    evidence: 'wardnetd/build.rs contains a near-identical generate_oui_data() to the one in wardnetd-data; crates/wardnetd/data/oui.csv duplicates crates/wardnetd-data/data/oui.csv; no include! of the generated file in wardnetd/src.',
    failure_scenario: 'Dead build cost on every compile of the main binary, plus a duplicated 39k-row CSV in the repo. No runtime impact.',
    proposed_action: 'Delete generate_oui_data() from wardnetd/build.rs and remove crates/wardnetd/data/oui.csv, after confirming nothing includes the generated output.',
    confidence: 'medium',
    unit: 'daemon-infra-linux',
    axis: 'design',
    component: ['component:daemon'],
    id: 'seed-oui-deadcode',
  },
]

const PROD_AXES = [
  { key: 'correctness', model: 'opus' },
  { key: 'security', model: 'opus' },
  { key: 'design', model: 'sonnet' },
  { key: 'conventions', model: 'sonnet' },
]

// Smoke mode: one small unit, one axis. Exercises every phase end-to-end cheap.
const SELECTED = SMOKE
  ? UNITS.filter((u) => u.key === 'sdk')
  : A.units && A.units.length
    ? UNITS.filter((u) => A.units.includes(u.key))
    : UNITS

const axesFor = (unit) => {
  if (unit.lens === 'test-quality') return [{ key: 'test-quality', model: 'sonnet' }]
  return SMOKE ? [{ key: 'correctness', model: 'opus' }] : PROD_AXES
}

// ---------------------------------------------------------------------------
// Schemas
// ---------------------------------------------------------------------------

const FINDINGS_SCHEMA = {
  type: 'object',
  required: ['unit', 'axis', 'findings', 'coverage_note'],
  properties: {
    unit: { type: 'string' },
    axis: { type: 'string' },
    coverage_note: {
      type: 'string',
      description: 'Honestly: what did you actually read, and what did you not get to? This feeds the gaps section of the report.',
    },
    findings: {
      type: 'array',
      items: {
        type: 'object',
        required: ['severity', 'category', 'file', 'issue', 'evidence', 'failure_scenario', 'proposed_action', 'confidence'],
        properties: {
          severity: {
            type: 'string',
            enum: ['RED', 'AMBER', 'GREEN'],
            description: 'RED = correctness/security defect with real consequences. AMBER = should fix; degrades reliability, maintainability or contract. GREEN = nit; style, naming, minor duplication.',
          },
          category: {
            type: 'string',
            description: 'axis:subtype, e.g. security:missing-auth-guard, correctness:race, design:layering, conventions:test-layout',
          },
          file: { type: 'string', description: 'Repo-relative path.' },
          line: { type: 'string', description: 'Line or range. "n/a" only if genuinely file-level.' },
          issue: { type: 'string', description: 'One sentence. The defect, not the fix.' },
          evidence: { type: 'string', description: 'Quote the actual code. A reader must be able to confirm this without opening the file.' },
          failure_scenario: {
            type: 'string',
            description: 'Concrete: which input / caller / sequence, and what breaks. "Could theoretically panic" is NOT a failure scenario. For conventions findings, quote the rule being violated instead.',
          },
          proposed_action: { type: 'string', description: 'The concrete fix.' },
          confidence: { type: 'string', enum: ['high', 'medium', 'low'] },
        },
      },
    },
  },
}

const VERDICT_SCHEMA = {
  type: 'object',
  required: ['refuted', 'reasoning', 'confidence'],
  properties: {
    refuted: { type: 'boolean', description: 'true = this finding does not hold up. Default to true when uncertain.' },
    reasoning: { type: 'string', description: 'What you checked and what you concluded. Cite the code you read.' },
    corrected_severity: {
      type: 'string',
      enum: ['RED', 'AMBER', 'GREEN'],
      description: 'If real but mis-severitied, the severity it should be.',
    },
    release_blocker: {
      type: 'boolean',
      description: 'True ONLY if a VERIFIED concrete scenario shows: an under-privileged caller reaching admin functions or another device\'s data; corruption/loss of config, tunnels or the DB; or a daemon panic/wedge triggerable by input any LAN device can send. Theoretical does not count.',
    },
    release_blocker_reason: { type: 'string' },
    confidence: { type: 'string', enum: ['high', 'medium', 'low'] },
  },
}

const CLUSTER_SCHEMA = {
  type: 'object',
  required: ['clusters'],
  properties: {
    clusters: {
      type: 'array',
      items: {
        type: 'object',
        required: ['title', 'kind', 'priority', 'components', 'finding_ids', 'rationale'],
        properties: {
          title: { type: 'string', description: 'Reads like a GitHub issue title. Imperative. No "Review X" or "Investigate Y".' },
          kind: { type: 'string', enum: ['root-cause', 'component', 'polish', 'misc'] },
          priority: { type: 'string', enum: ['priority:P1', 'priority:P2', 'priority:P3'] },
          label_type: { type: 'string', enum: ['bug', 'enhancement'] },
          components: { type: 'array', items: { type: 'string' }, description: 'From the component: labels of the units involved.' },
          finding_ids: { type: 'array', items: { type: 'string' } },
          rationale: { type: 'string', description: 'Why these belong in ONE session together.' },
          release_blocker: { type: 'boolean' },
        },
      },
    },
  },
}

const ISSUE_SCHEMA = {
  type: 'object',
  required: ['title', 'body', 'labels'],
  properties: {
    title: { type: 'string' },
    body: { type: 'string', description: 'Full GitHub-flavored markdown issue body.' },
    labels: { type: 'array', items: { type: 'string' } },
    slug: { type: 'string', description: 'kebab-case, for the filename' },
  },
}

// ---------------------------------------------------------------------------
// Prompts
// ---------------------------------------------------------------------------

const SEVERITY = `SEVERITY:
- RED: a correctness or security defect with real consequences. Something breaks, leaks, corrupts, or lets the wrong caller in.
- AMBER: should be fixed. Degrades reliability, maintainability, or a contract, but nothing is on fire.
- GREEN: a nit. Style, naming, minor duplication.

THE STANDING ORDER: a finding without a concrete failure scenario is not a finding.
Name the input, the caller, or the sequence that makes it break, and say what breaks. "This could panic" / "this might race" / "this may be slow" are not findings — they are anxieties. Delete them before you return.
The one exception is CONVENTIONS findings, which have no runtime failure scenario by definition; for those, quote the documented rule being violated and where it is written.

Report NOTHING you have not read the code for. Do not infer a bug from a filename or a symbol. Open the file.
A short list of real findings is a SUCCESS. A long list padded with speculation wastes a human's afternoon and is a failure.`

const reviewPrompt = (unit, axis, digest) => {
  const axisBrief = {
    correctness: `Hunt LOGIC DEFECTS. Specifically:
- Unhandled edge cases and error paths that swallow failure (\`let _ =\`, ignored Results, empty catch).
- State that can desync between two places that must agree (cache vs source of truth, in-memory view vs DB).
- Races and ordering bugs: concurrent mutation, TOCTOU, event-handler reentrancy, assumptions about task ordering.
- Boundary conditions: empty, one, many, max; overflow; off-by-one; unbounded growth.
- Resource lifecycle: leaks, unclosed handles, tasks that outlive their purpose, unbounded queues/caches.
- Hot-path work that is genuinely wrong (an O(n) scan per DNS query, an await inside a lock, blocking I/O in async). Real hot-path defects belong here — but do NOT free-associate micro-optimizations. Without a profiler, speculative perf "findings" are noise; if you cannot name the request path and why it hurts, drop it.`,
    security: `Hunt SECURITY DEFECTS. Specifically:
- THE AUTH GUARD. This repo's hardest rule: EVERY service method must open with \`auth_context::require_admin()?\` or \`require_authenticated()?\` as its FIRST statement. The only permitted exception is a startup/restore path that runs before the system is ready, and it must carry a comment saying why. A missing guard on a reachable method is a RED and probably a release blocker — verify reachability from a handler and say so.
- Authorization vs authentication: can an authenticated NON-admin, or a device, reach admin functionality or ANOTHER device's data? Device-keyed identity is not an admin session (see CONTEXT.md).
- Untrusted input: this daemon parses DNS and DHCP packets from any device on the LAN. A panic on malformed input takes down the household's gateway. Trace unwrap/expect/slicing/parsing on those paths.
- Secrets: keys, tokens, passwords, WireGuard private keys reaching logs, errors, or API responses. Check the logging calls.
- Injection: SQL built with \`format!()\` instead of \`.bind()\`; shell/nftables commands built from user input.
- Crypto: weak/misused primitives, bad randomness, hand-rolled comparison of secrets.
- \`unsafe\` blocks and any UB.`,
    design: `Hunt DESIGN DEFECTS — things that make the next change harder. Specifically:
- Layering violations. The layers are real and documented: handlers stay thin and logic lives in services; repositories own SQL; no SQL in handlers. On the web side: core/ui → compound → features → layouts → pages, and ONLY pages may call the API.
- Responsibility in the wrong place; a module that knows about something it should not.
- Duplication that has diverged — the same logic in two places where one has a fix the other lacks. (Trivial repetition is not a finding.)
- Leaky abstractions: a trait whose impls cannot honour its contract; an interface that forces callers to know the impl.
- Dead code, dead flags, abandoned half-migrations.
Do NOT propose a rewrite. Do not report "this could be more elegant". Report what will actually bite.`,
    conventions: `Hunt violations of THIS REPO'S DOCUMENTED RULES. Not generic best practice — the rules in the digest above, which come from the repo's own .agents/ docs.
The high-value ones:
- Tests must live in separate files. NO \`#[test]\` / \`#[tokio::test]\` inline in a source file — layout is \`src/<layer>/tests/<module>.rs\`.
- SQL is \`const\` query strings + \`.bind()\`. \`format!()\` only for PRAGMA with numeric constants.
- Every handler carries \`#[utoipa::path(...)]\`; DTOs derive \`ToSchema\`; \`Ipv4Addr\`/\`IpAddr\` need \`#[schema(value_type = String)]\`.
- Every dependency has a comment with its crates.io / npmjs URL above it.
- Doc comments on every public trait/struct/enum; \`#[must_use]\` on pure accessors.
- Web: typography ONLY via \`<Text>\`/\`<Heading>\` variants — no raw \`text-*\`/\`font-*\`; \`react-router\` not \`react-router-dom\`; shared hooks live in \`@wardnet/web\`; detail views are routed pages, not sheets.
For each finding, QUOTE the rule and name the doc it comes from. A convention finding that cannot cite a rule is not a convention finding — drop it.
If the code contradicts a rule and you suspect the RULE is the stale thing rather than the code, say so explicitly in the issue text; do not silently pick a side.`,
    'test-quality': `You are reviewing TEST CODE as code. The question is not "do these pass" — it is "would these CATCH a regression".
- Assertions that cannot fail: asserting on a mock's return value, \`assert!(true)\`, asserting a function was called rather than that it did the right thing, snapshot tests with no meaningful content.
- Tests that mock the very thing under test, so the test passes no matter what the real code does.
- Missing coverage of the paths that MATTER: error branches, auth-guard rejection, boundary values, concurrent access — not line count, but risk coverage.
- Flakiness sources: real sleeps, wall-clock/timing dependence, ordering dependence between tests, shared mutable fixtures, network/filesystem reliance.
- Playwright: locators that are not \`data-testid\` (ADR 0015 mandates it), and waits on timeouts rather than conditions.
Report the tests whose ABSENCE OF VALUE is demonstrable. Quote the assertion that cannot fail.`,
  }[axis.key]

  return `You are reviewing one unit of the Wardnet codebase on ONE axis: ${axis.key.toUpperCase()}.

REPO (read-only, do not modify anything): ${TARGET}
UNIT: ${unit.key}
WHAT IT IS: ${unit.role}
PATHS (repo-relative):
${unit.paths.map((p) => `  - ${p}`).join('\n')}
${unit.context ? `\nSUBSYSTEM CONTEXT (read this first): ${unit.context}\n` : ''}
IGNORE these paths entirely — generated, vendored, or checked-in build output:
${EXCLUDES.map((e) => `  - ${e}`).join('\n')}

=== THIS REPO'S HOUSE RULES (distilled from its own .agents/ docs) ===
${digest}
=== END HOUSE RULES ===

YOUR AXIS — ${axis.key}:
${axisBrief}

${SEVERITY}

METHOD:
1. Orient: list the files in the unit, read the module entry points, understand what this unit is FOR before judging it.
2. Read the code. Follow the call paths that matter — a service method's callers, a handler's route, where a value flows to. You are not reviewing a diff; you can and must trace across files.
3. Where you suspect a defect, CONFIRM it by reading the code that would catch it. Half of all plausible findings die here — that is the point.
4. Report what survives.

This is a whole-unit review, so you can find what a diff review structurally cannot: the guard that was never added, the error branch nobody wrote, the case nobody handles. Look for the MISSING as hard as you look for the wrong.
${EXTRA ? `\n=== EXTRA INSTRUCTION FOR THIS RUN ===\n${EXTRA}\n` : ''}
Return the structured object. Empty findings is a legitimate, respectable result.`
}

const sweepPrompt = (sweep, digest) => `You are running ONE repo-wide invariant sweep across the whole Wardnet codebase.

REPO (read-only): ${TARGET}
IGNORE: ${EXCLUDES.join(', ')}

=== HOUSE RULES ===
${digest}
=== END ===

YOUR SWEEP: ${sweep.name}
${sweep.brief}

This is a MECHANICAL, EXHAUSTIVE check — that is its whole value. A per-unit reviewer sees one subsystem; you see every violation of one rule, everywhere. Be systematic: grep/glob to enumerate every candidate site, then READ each candidate to confirm before reporting. Grep finds candidates; it does not find violations. Confirm every one.

State in coverage_note how many candidate sites you enumerated and how many you confirmed — the ratio matters to whoever reads this.

${SEVERITY}

Set unit to "sweep:${sweep.key}" and axis to "${sweep.axis}".
Return the structured object.`

const SWEEPS = [
  {
    key: 'auth-guard',
    name: 'The auth-guard HARD REQUIREMENT',
    axis: 'security',
    brief: `.agents/auth.md: "Every service method MUST validate the authentication context as its FIRST operation using auth_context::require_admin()?; or auth_context::require_authenticated()?;. Services never trust their caller."
Enumerate EVERY service trait method impl in source/daemon/crates/wardnetd-services/. For each, read the first statement of the body.
Violation = the guard is absent, or is not first, or the method is reachable from an HTTP handler without one.
The ONE permitted exception: startup/restore methods that run before the system is ready (e.g. restore_tunnels) — and .agents/auth.md requires those to carry a COMMENT explaining why the guard is skipped. An undocumented exception is still a violation; report it as one.
For each violation, establish REACHABILITY: trace whether an HTTP handler can reach it. A reachable unguarded method is RED and likely a release blocker. An unreachable one is still AMBER.
This is the single highest-value sweep in the review. Be exhaustive.`,
  },
  {
    key: 'test-layout',
    name: 'Tests must live in separate files',
    axis: 'conventions',
    brief: `.agents/testing.md: tests MUST live in separate files — "Never put #[test] or #[tokio::test] blocks inline in source files". Layout is src/<layer>/tests/<module>.rs with #[cfg(test)] mod tests; in the layer's mod.rs. "This rule is enforced by clippy and code review — no exceptions."
Find every #[test] / #[tokio::test] that sits OUTSIDE a **/tests/** path. Report each file. Severity GREEN unless it is widespread, in which case one AMBER for the pattern.`,
  },
  {
    key: 'sql-injection',
    name: 'SQL construction',
    axis: 'security',
    brief: `Rule: SQL is const query strings + .bind(). format!() is permitted ONLY for PRAGMA with numeric constants.
Find every SQL string in source/daemon/crates/wardnetd-data/ built with format!(), string concatenation, or interpolation. For each, determine whether any component derives from user/network input. If yes: RED, injection. If it is a PRAGMA with a numeric constant: not a finding. Also flag any SQL that has escaped the repository layer into a handler or service.`,
  },
  {
    key: 'openapi-annotations',
    name: 'OpenAPI annotation completeness',
    axis: 'conventions',
    brief: `Rule: every handler carries #[utoipa::path(...)] with method/path/tag/description/request_body/responses/security; every DTO derives ToSchema; Ipv4Addr/IpAddr fields need #[schema(value_type = String)].
Enumerate every axum route registered in wardnetd-api and check its handler for a complete annotation. Report handlers with a missing annotation, and annotations missing the security field (that last one is a real bug: it makes an authenticated endpoint look public in the published spec — AMBER, or RED if it also lacks a guard).`,
  },
  {
    key: 'panic-paths',
    name: 'Panics reachable from untrusted input',
    axis: 'security',
    brief: `This daemon is a household gateway. It parses DNS and DHCP packets from any device on the LAN. A panic on malformed input takes the internet down for the house, and the watchdog will restart or reboot it.
Find every unwrap() / expect() / panic!() / unreachable!() / array-index / slice-range in the PRODUCTION packet-handling paths: wardnetd/src (dns, dhcp, netlink, nftables, wireguard) and the dns/dhcp/ddns services.
For each, ask: can a LAN device or a network response drive it? TRACE IT. Report only the ones you can actually connect to hostile input — a panic on a value the code itself just constructed is not a finding. The reachable ones are RED and are release-blocker candidates. Ignore unwraps in tests entirely.`,
  },
  {
    key: 'dep-docs',
    name: 'Dependency URL comments',
    axis: 'conventions',
    brief: `Rule (.agents/code-conventions.md): "Always add a comment with the crates.io or npmjs URL before each dependency."
Check every dependency in source/daemon/**/Cargo.toml and every package.json in source/. Report the ones missing the URL comment. This is GREEN — aggregate them into ONE finding per manifest, not one per dependency. Do not turn 80 crates into 80 findings.`,
  },
  {
    key: 'web-layering',
    name: 'Web-UI layering and typography rules',
    axis: 'conventions',
    brief: `Rules (.agents/code-conventions.md): component layers core/ui → compound → features → layouts → pages, and ONLY pages may call the API; typography ONLY via <Text>/<Heading> variant props — no raw text-*/font-* Tailwind classes; import from react-router, NEVER react-router-dom; shared hooks belong in @wardnet/web, not app-local hooks/ dirs; detail views are routed pages, not sheets.
Sweep source/admin-site/src, source/web/src, source/admin-app/src, source/user-app/src. Report each class of violation. Aggregate repetitive ones (e.g. raw text-* classes) into one finding per app with a representative file list and a count — not one finding per line.`,
  },
]

const verifyPrompt = (f, lens) => `You are a SKEPTIC. Your job is to REFUTE a code-review finding, not to confirm it.

REPO (read-only): ${TARGET}

THE FINDING:
${JSON.stringify(f, null, 2)}

YOUR LENS: ${lens.name}
${lens.brief}

METHOD: open the cited file. Read the surrounding code, the callers, and anything that would ALREADY prevent this. Then try to kill it. The finding dies if:
- The claimed failure scenario cannot actually occur (a guard, a type, an invariant, a caller contract already prevents it).
- The evidence misreads the code.
- It is in test code, generated code, or a path that does not run in production.
- The severity is inflated: a theoretical panic with no reachable trigger is not RED.
- It is a known, documented, deliberate limitation of this system rather than a defect. (Example: same-subnet peer traffic being unaffected by zone enforcement is DOCUMENTED and intentional — refute it.)
- It is speculative: "could", "might", "may be slow", with no named trigger.

DEFAULT TO REFUTED. If you cannot construct the concrete failure yourself by reading the code, it is refuted. The cost of a false finding is a human wasting a session on a phantom bug; the cost of dropping a marginal one is low, because a real bug will be found again.

ONE CARVE-OUT — do NOT refute a CONVENTIONS or documentation-drift finding merely for lacking a runtime failure scenario. By definition it has none. Judge those ONLY on: does the cited rule actually exist in this repo's docs, and does the code actually violate it? If both are true, it stands.

If the finding is REAL, also judge RELEASE BLOCKER — true only if a verified, concrete scenario shows one of:
- an unauthenticated or under-privileged caller reaching admin functionality or another device's data;
- corruption or loss of config, tunnels, or the database;
- a daemon panic/wedge/crash-loop triggerable by input any LAN device can send.
Theoretical does not qualify. You must have traced it yourself.

Return the structured verdict.`

const LENSES = [
  { key: 'reachability', name: 'Reachability', brief: 'Can this code even be reached in production with the claimed input? Trace from the entry point — a handler, a packet parser, a runner. If nothing can drive it, it is refuted.' },
  { key: 'already-handled', name: 'Already handled', brief: 'Is there already a guard, type invariant, validation, or caller contract that prevents this? Look upstream of the cited line. Most plausible findings die here.' },
  { key: 'exploitability', name: 'Exploitability / consequence', brief: 'Grant the mechanism. What ACTUALLY happens? Who has to do what to trigger it, and how bad is the result? If triggering requires privileges the attacker would already need for a worse outcome, the finding is inflated at best.' },
]

const clusterPrompt = (findings) => `You are turning ${findings.length} verified code-review findings into GitHub issues that a developer will fix in FUTURE, SEPARATE working sessions.

THE FINDINGS:
${JSON.stringify(findings, null, 2)}

Produce 15-30 clusters. Each cluster becomes ONE issue. The test of a good cluster: could one developer sit down and close it in one focused session, with everything they need in front of them?

CLUSTER BY ROOT CAUSE FIRST, component second.
- "Missing auth guard on 6 service methods" is ONE issue, ONE session, one mental model, one PR — not six issues.
- Unrelated bugs that merely happen to live in the same file are NOT a cluster.
- If a group is too big for one session, split it along a natural seam and say so in the rationale.

KINDS:
- root-cause: several findings sharing one underlying cause or one fix. Prefer this.
- component: related-but-distinct findings in one area that make sense to fix together.
- polish: GREEN nits, aggregated into ONE issue per app, as a checklist. Drop an app's polish issue entirely if it would have fewer than 5 items — a tracker rots on trivia.
- misc: genuine orphans, one per component. Use sparingly; a large misc issue means you gave up on clustering.

PRIORITY (this is a strict mapping — do NOT invent priority:P0):
- RED → priority:P1
- AMBER → priority:P2
- GREEN → priority:P3
A cluster takes the priority of its highest-severity member.
Set release_blocker: true on a cluster if ANY member was verified as a release blocker. That flag surfaces it to a human for a P0 decision — the workflow does not make that call itself.

TITLES: imperative, specific, and about the FIX. "Add missing auth guards to DNS filter service methods" — not "Auth issues in DNS" and not "Review the DNS service".

label_type: bug for defects, enhancement for hardening/refactor/test work.

Return the structured object. Every finding id must land in exactly one cluster.`

const issuePrompt = (cluster, findings) => `Write ONE GitHub issue. It will be filed in the Wardnet repo and picked up in a future session by a developer who was NOT part of this review and has none of its context.

CLUSTER: ${JSON.stringify(cluster, null, 2)}

ITS FINDINGS (verbatim, verified):
${JSON.stringify(findings, null, 2)}

The body, in GitHub-flavored markdown:

## Problem
What is actually wrong and why it matters. Concrete — the failure, not the abstraction. If a user or the household is affected, say how. 2-4 sentences.

## Findings
A checklist, one item per finding:
- [ ] \`path/to/file.rs:123\` — the defect in one line, with the key evidence quoted inline.
Every item carries a real file:line. A developer must be able to jump straight to it.

## Proposed fix
The concrete approach for the cluster as a whole. Where findings need different fixes, say so per finding. Do not hand-wave, and do not write the patch — this is a brief, not a PR.

## Acceptance criteria
Checkboxes a reviewer can objectively verify. Include the test that must exist: this repo mandates that new code has tests and that coverage never decreases (.agents/workflow.md).

## Notes
Anything the fixer needs: the repo rule being violated and where it is documented, the subsystem doc to read first, any adjacent code that must not regress.

---
Found by \`full-repo-review\` ${DATE} · main @ ${SHA}${cluster.release_blocker ? '\n\n> **Flagged as a possible release blocker** — a verified scenario shows security, data-loss, or gateway-availability impact. Priority needs a human call.' : ''}

LABELS — use these EXACT strings, verbatim, including the \`component:\` prefix. These are real labels in the repo's taxonomy; a label you invent or abbreviate will fail at \`gh issue create\`. Do NOT strip prefixes, do NOT reword, do NOT add labels that are not listed here:
${JSON.stringify([...(cluster.components || []), cluster.priority, cluster.label_type || 'bug', 'review'])}

TONE: plain, direct, no filler. No "this is a great opportunity to improve". State the defect, the fix, the done-condition. Assume a competent reader with zero context on this review.

Return the structured object. slug is kebab-case, derived from the title, max 6 words.`

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

log(`${SMOKE ? 'SMOKE' : 'FULL'} run · target ${TARGET} @ ${SHA} · ${SELECTED.length} unit(s)`)

// --- 1. house rules digest -------------------------------------------------
phase('Digest')
const digest = await agent(
  `Read this repo's agent-facing convention docs and distil them into a HOUSE RULES digest that will be injected into ~90 code-reviewer agents.

READ THE DOCS FROM: ${DOCS_FROM}
(Read AGENTS.md, CONTEXT.md, and every file in .agents/ from THAT path — especially auth.md, code-conventions.md, testing.md, workflow.md, architecture.md. These docs were just audited and corrected; the copies in the code worktree may still be stale, so do not read them from anywhere else.)

The CODE being reviewed lives at ${TARGET}. You do not need to read it — your job is only the rules.

Produce a dense digest (aim 800-1200 words) that a reviewer can hold in its head:
1. THE HARD RULES — rules this project treats as non-negotiable, quoted, each with the doc it comes from. The auth-guard requirement is the flagship; there are others (test file layout, SQL binding, OpenAPI annotations, dependency URL comments, web typography/layering).
2. ARCHITECTURE IN BRIEF — the layers, what belongs in each, and what must never cross them.
3. DELIBERATE DESIGN DECISIONS THAT LOOK LIKE BUGS — things a naive reviewer will "find" that are actually intentional and documented. Example: zone enforcement does not affect same-subnet peer traffic (that is the AP's job, and it is documented). Another: the hardware watchdog pet is deliberately NEVER health-gated. Call these out explicitly so reviewers don't waste a run rediscovering them.
4. DOMAIN VOCABULARY — from CONTEXT.md: the three app surfaces, device-keyed vs admin-session identity, the infrastructure terms.

Be concrete and quote the rules. This digest is the only view of the repo's conventions that most reviewers will get. Return the digest as plain markdown text — no preamble, no "here is the digest".`,
  { label: 'house-rules', phase: 'Digest', model: 'opus' }
)

if (!digest) throw new Error('conventions digest failed — every downstream reviewer depends on it')

// --- 2. invariant sweeps (parallel with unit review; they share the cap) ----
phase('Sweep')
const sweepsToRun = SKIP_SWEEPS ? [] : SMOKE ? SWEEPS.slice(0, 1) : SWEEPS
const sweepPromise = parallel(
  sweepsToRun.map((s) => () =>
    agent(sweepPrompt(s, digest), {
      label: `sweep:${s.key}`,
      phase: 'Sweep',
      schema: FINDINGS_SCHEMA,
      model: 'opus',
      effort: 'high',
    })
  )
)

// --- 3+4. unit review → adversarial verify (pipelined, no barrier) ---------
phase('Review')

const verifyFinding = async (f) => {
  const isConvention = /^(conventions|docs)/.test(f.category || '') || f.axis === 'conventions'
  if (f.severity === 'GREEN' && f.confidence === 'low') return null // house rule: discard outright
  if (f.severity === 'GREEN') return { ...f, verdict: { refuted: false, reasoning: 'GREEN nit — not verified', confidence: 'low' } }

  // RED gets three perspective-diverse skeptics; AMBER gets one. Conventions
  // findings get one, and the lens is told not to demand a failure scenario.
  const lenses = isConvention ? [LENSES[1]] : f.severity === 'RED' ? LENSES : [LENSES[1]]
  const votes = (
    await parallel(lenses.map((l) => () => agent(verifyPrompt(f, l), { label: `verify:${l.key}`, phase: 'Verify', schema: VERDICT_SCHEMA, model: 'opus' })))
  ).filter(Boolean)

  if (!votes.length) return null
  const refuted = votes.filter((v) => v.refuted).length
  const survives = refuted < votes.length / 2 // majority must NOT refute
  const winner = votes.find((v) => !v.refuted) || votes[0]

  return {
    ...f,
    severity: winner.corrected_severity || f.severity,
    release_blocker: votes.some((v) => v.release_blocker === true),
    release_blocker_reason: (votes.find((v) => v.release_blocker) || {}).release_blocker_reason,
    verdict: { refuted: !survives, reasoning: votes.map((v) => v.reasoning).join(' | '), votes: votes.length, refuted_by: refuted },
  }
}

const reviewed = await pipeline(
  SELECTED,
  (unit) =>
    parallel(
      axesFor(unit).map((axis) => () =>
        agent(reviewPrompt(unit, axis, digest), {
          label: `${unit.key}:${axis.key}`,
          phase: 'Review',
          schema: FINDINGS_SCHEMA,
          model: axis.model,
          effort: 'high',
        })
      )
    ),
  (results, unit) => {
    const rs = (results || []).filter(Boolean)
    const fs = rs.flatMap((r) =>
      (r.findings || []).map((f, i) => ({
        ...f,
        id: `${unit.key}-${r.axis}-${i}`,
        unit: unit.key,
        axis: r.axis,
        component: unit.component,
      }))
    )
    return parallel(fs.map((f) => () => verifyFinding(f))).then((v) => ({
      unit: unit.key,
      coverage: rs.map((r) => `${r.axis}: ${r.coverage_note}`),
      findings: v.filter(Boolean),
    }))
  }
)

const sweepResults = (await sweepPromise).filter(Boolean)

// Sweep findings go through the same skeptics — a mechanical sweep is exactly
// the thing that produces confident false positives.
phase('Verify')
const sweepFindings = sweepResults.flatMap((r, ri) =>
  (r.findings || []).map((f, i) => ({ ...f, id: `sweep-${sweepsToRun[ri].key}-${i}`, unit: r.unit, axis: r.axis, component: ['component:daemon'] }))
)
const sweepVerified = (await parallel(sweepFindings.map((f) => () => verifyFinding(f)))).filter(Boolean)

// Seeds from the docs-accuracy run get the same skeptics as everything else.
const seedsToVerify = SMOKE || SKIP_SEEDS ? [] : SEEDS
const seedVerified = (await parallel(seedsToVerify.map((f) => () => verifyFinding(f)))).filter(Boolean)
if (seedsToVerify.length) log(`${seedVerified.filter((f) => !f.verdict.refuted).length}/${seedsToVerify.length} docs-audit seed findings survived verification`)

const all = [...reviewed.filter(Boolean).flatMap((r) => r.findings), ...sweepVerified, ...seedVerified]
const survivors = all.filter((f) => !f.verdict.refuted)
const refutedList = all.filter((f) => f.verdict.refuted)
const blockers = survivors.filter((f) => f.release_blocker)

log(`${all.length} findings · ${survivors.length} survived · ${refutedList.length} refuted · ${blockers.length} release-blocker candidate(s)`)

if (!survivors.length) {
  return { survivors: 0, refuted: refutedList.length, note: 'Nothing survived verification. No issues drafted.' }
}

// --- 5. cluster (barrier: needs every survivor at once) --------------------
phase('Cluster')
const clustered = await agent(clusterPrompt(survivors), { label: 'cluster', phase: 'Cluster', schema: CLUSTER_SCHEMA, model: 'opus', effort: 'high' })
const clusters = (clustered && clustered.clusters) || []
log(`${clusters.length} clusters → ${clusters.length} draft issues`)

// --- 6. draft one issue per cluster ---------------------------------------
phase('Draft')
const byId = new Map(survivors.map((f) => [f.id, f]))
const issues = (
  await parallel(
    clusters.map((c, i) => () =>
      agent(issuePrompt(c, (c.finding_ids || []).map((id) => byId.get(id)).filter(Boolean)), {
        label: `issue:${(c.title || 'untitled').slice(0, 32)}`,
        phase: 'Draft',
        schema: ISSUE_SCHEMA,
        model: 'sonnet',
      }).then((iss) => (iss ? { ...iss, n: i + 1, cluster: c } : null))
    )
  )
).filter(Boolean)

// --- 7. what did we miss? --------------------------------------------------
phase('Critic')
const critic = await agent(
  `You are auditing a code review for COMPLETENESS. Be uncharitable.

REPO: ${TARGET}
UNITS REVIEWED (${SELECTED.length}): ${SELECTED.map((u) => u.key).join(', ')}
AXES PER PRODUCTION UNIT: ${PROD_AXES.map((a) => a.key).join(', ')}
SWEEPS RUN: ${sweepsToRun.map((s) => s.key).join(', ')}
FINDINGS: ${all.length} raised, ${survivors.length} survived verification, ${refutedList.length} refuted.

WHAT EACH REVIEWER SAID IT ACTUALLY COVERED:
${JSON.stringify(reviewed.filter(Boolean).map((r) => ({ unit: r.unit, coverage: r.coverage })), null, 2)}

Answer, concretely:
1. What code in this repo was NOT reviewed by any unit? Check the actual tree against the unit paths above — name real directories that fell through the gaps.
2. What class of defect could NO axis and NO sweep have caught, given the axes above?
3. Where does the coverage look SUSPICIOUSLY thin — a large unit that produced almost nothing, a reviewer whose coverage_note admits it only skimmed?
4. Did the verify phase look over-aggressive? A high refutation rate on one axis can mean the skeptics are killing real findings.
5. What would you review NEXT, and why?

Return plain markdown. Be specific and short. This becomes the "what this review did not look at" section of the report — its purpose is to stop the reader believing the review was more complete than it was.`,
  { label: 'critic', phase: 'Critic', model: 'opus' }
)

// --- 8. write the deliverables --------------------------------------------
phase('Write')
const written = await agent(
  `Write the deliverables of a full-repo code review. You are the only agent that writes to disk. Write ONLY inside ${OUT}.

Create ${OUT}/ and ${OUT}/issues/, then write:

1. ${OUT}/report.md
2. ${OUT}/issues/NNN-<slug>.md — one per issue below, NNN zero-padded (001, 002...), body EXACTLY as given. These are --body-file inputs for \`gh issue create\`; they must contain the issue body and nothing else.
3. ${OUT}/issues/manifest.json — [{ "n", "file", "title", "labels": [...], "priority", "release_blocker" }], one entry per issue, valid JSON.

RUN: ${DATE} · main @ ${SHA} · mode=${MODE} · units=${SELECTED.map((u) => u.key).join(', ')}

ISSUES TO WRITE (${issues.length}):
${JSON.stringify(issues, null, 2)}

RELEASE-BLOCKER CANDIDATES (${blockers.length}):
${JSON.stringify(blockers, null, 2)}

ALL SURVIVING FINDINGS (${survivors.length}):
${JSON.stringify(survivors, null, 2)}

REFUTED (${refutedList.length}):
${JSON.stringify(refutedList, null, 2)}

COMPLETENESS CRITIC:
${critic}

report.md STRUCTURE — a human reads this to decide what to file, so lead with what demands a decision:

# Wardnet full-repo review — ${DATE}
main @ ${SHA} · ${SELECTED.length} units · ${all.length} findings raised, ${survivors.length} survived, ${refutedList.length} refuted

## Release-blocker candidates
Each with its verified failure scenario and which draft issue covers it. If there are none, say "None." in one line — do not manufacture drama.

## Draft issues
Table: # | title | priority | components | findings. This is the shopping list.

## Findings by unit
Per unit: a table of surviving findings (severity | file:line | issue). Compact.

## Refuted findings
<details>-collapsed. Table: finding | why it was refuted. This exists so we can audit whether the skeptics are too aggressive.

## What this review did not look at
The critic's output, verbatim.

RULES:
- Do not editorialize or inflate. If the review found little, say so plainly.
- Every finding shows a real file:line.
- Do NOT run gh. Do NOT create issues. Do NOT touch anything outside ${OUT}.

When done, reply with ONLY: the report path, the issue count, and the release-blocker count.`,
  { label: 'scribe', phase: 'Write', model: 'sonnet' }
)

return {
  mode: MODE,
  target: `main @ ${SHA}`,
  units: SELECTED.length,
  findings_raised: all.length,
  survived: survivors.length,
  refuted: refutedList.length,
  release_blockers: blockers.length,
  issues_drafted: issues.length,
  out: OUT,
  scribe: written,
}
