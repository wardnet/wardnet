export const meta = {
  name: 'docs-accuracy',
  description: 'Verify AGENTS.md / CONTEXT.md / .agents/*.md against the real tree; report drift, change nothing',
  whenToUse:
    'Run BEFORE full-repo-review. The code review builds a conventions digest from these docs and injects it into every reviewer — if the docs are stale, every reviewer inherits the drift. Reports only; a human decides the fixes.',
  phases: [
    { title: 'Audit', detail: 'one agent per doc, verify every checkable claim against the tree' },
    { title: 'Synthesize', detail: 'cross-doc contradictions + needs-your-call list' },
    { title: 'Write', detail: 'scribe writes docs-report.md' },
  ],
}

// ---------------------------------------------------------------------------
// Config. `args` = { date, target, docs? }
// ---------------------------------------------------------------------------

// The host may hand `args` through as a JSON string rather than an object.
const A = typeof args === 'string' ? JSON.parse(args) : args || {}

// SKILL.md always passes an absolute `target` (the worktree to review). The
// relative fallback only exists so the script is runnable ad hoc from the repo root.
const TARGET = A.target || 'main'
const DATE = A.date || 'undated'
const OUT = `${TARGET}/.reviews/${DATE}`

// Every doc that feeds the conventions digest. One agent each.
const ALL_DOCS = [
  { path: 'AGENTS.md', role: 'Root index. Points at .agents/* and CONTEXT.md. Also states the agent-memory path rule.' },
  { path: 'CONTEXT.md', role: 'Canonical domain glossary — app surfaces, identity model, infrastructure, planned features.' },
  { path: '.agents/architecture.md', role: 'Layered design, trait boundaries, and every subsystem deep-dive (stats, local-DNS, DDNS, watchdog, zone enforcement).' },
  { path: '.agents/project-structure.md', role: 'Full source tree with a one-line purpose per module. KNOWN SUSPECT: describes source/ui and source/styles as in-repo packages.' },
  { path: '.agents/technical-stack.md', role: 'Versions and key dependencies per app.' },
  { path: '.agents/code-conventions.md', role: 'Rust / SDK / web style rules, OpenAPI annotation pattern, dependency-doc format.' },
  { path: '.agents/auth.md', role: 'The auth model and the HARD REQUIREMENT that every service method opens with require_admin()/require_authenticated().' },
  { path: '.agents/testing.md', role: 'Test layout rules (tests in separate files) and mock/real-resource patterns.' },
  { path: '.agents/workflow.md', role: 'Git conventions, mandatory pre-push checklist, coverage rules, always/ask/never boundaries.' },
  { path: '.agents/commands.md', role: 'make targets and the direct cargo/yarn equivalents, per area.' },
  { path: '.agents/observability.md', role: 'Tracing span hierarchy for background components, OUI database, versioning.' },
  { path: '.agents/logging.md', role: 'How to write a log line that is queryable in Loki and readable in stderr.' },
  { path: '.agents/backup.md', role: 'BackupArchiver / DatabaseDumper / SecretStore composition, two-phase apply, cleanup runner.' },
]

const DOCS = A.docs && A.docs.length ? ALL_DOCS.filter((d) => A.docs.includes(d.path)) : ALL_DOCS

// ---------------------------------------------------------------------------
// Schemas
// ---------------------------------------------------------------------------

const DRIFT_SCHEMA = {
  type: 'object',
  required: ['doc', 'verdict', 'drifts', 'verified_claims'],
  properties: {
    doc: { type: 'string' },
    verdict: {
      type: 'string',
      enum: ['accurate', 'minor-drift', 'significant-drift', 'substantially-wrong'],
      description: 'Overall state of this doc.',
    },
    summary: { type: 'string', description: 'One or two sentences: is this doc trustworthy as a basis for code review?' },
    verified_claims: {
      type: 'integer',
      description: 'How many checkable claims you actually verified against the tree. Be honest; this is the denominator for the drift rate.',
    },
    drifts: {
      type: 'array',
      items: {
        type: 'object',
        required: ['classification', 'doc_says', 'reality', 'evidence', 'confidence'],
        properties: {
          classification: {
            type: 'string',
            enum: ['doc-stale', 'code-drifted', 'ambiguous'],
            description:
              'doc-stale = the doc is out of date and the code is fine; fix the doc. code-drifted = the rule is still right and the CODE broke it; this becomes a code finding for run B. ambiguous = the doc describes an intent the code never implemented, and you cannot tell which side is wrong WITHOUT A HUMAN DECIDING. Do not guess.',
          },
          section: { type: 'string', description: 'Heading or line reference inside the doc.' },
          doc_says: { type: 'string', description: 'What the doc claims. Quote it.' },
          reality: { type: 'string', description: 'What the tree actually contains.' },
          evidence: { type: 'string', description: 'The concrete check that proves it: the path that does/does not exist, the grep and its result, the file:line. Not a vibe.' },
          proposed_correction: {
            type: 'string',
            description: 'For doc-stale ONLY: the corrected text, ready to paste. Omit for code-drifted and ambiguous.',
          },
          why_it_matters: {
            type: 'string',
            description: 'What a code reviewer would get WRONG if it trusted this claim. If the honest answer is "nothing", say so — trivia is not drift.',
          },
          confidence: { type: 'string', enum: ['high', 'medium', 'low'] },
        },
      },
    },
  },
}

const SYNTH_SCHEMA = {
  type: 'object',
  required: ['contradictions', 'needs_your_call', 'digest_risk'],
  properties: {
    contradictions: {
      type: 'array',
      description: 'Places where two docs disagree with EACH OTHER (not with the code). These are invisible to a per-doc agent.',
      items: {
        type: 'object',
        required: ['docs', 'conflict', 'evidence'],
        properties: {
          docs: { type: 'array', items: { type: 'string' } },
          conflict: { type: 'string' },
          evidence: { type: 'string' },
          recommendation: { type: 'string' },
        },
      },
    },
    needs_your_call: {
      type: 'array',
      description: 'Every ambiguous drift, restated as a crisp question with a recommended answer. This is the section the human actually reads.',
      items: {
        type: 'object',
        required: ['question', 'recommendation'],
        properties: {
          doc: { type: 'string' },
          question: { type: 'string', description: 'A yes/no or this-or-that question. Not "what should we do about X".' },
          context: { type: 'string' },
          recommendation: { type: 'string' },
        },
      },
    },
    handoff_to_run_b: {
      type: 'array',
      description: 'The code-drifted items, restated as seed findings for the code review.',
      items: {
        type: 'object',
        required: ['rule', 'violated_where'],
        properties: {
          rule: { type: 'string' },
          source_doc: { type: 'string' },
          violated_where: { type: 'string' },
        },
      },
    },
    digest_risk: {
      type: 'string',
      enum: ['safe', 'fix-first', 'blocked'],
      description:
        'safe = run B can build its conventions digest from these docs as-is. fix-first = fix the doc-stale items first, they would mislead reviewers. blocked = the docs are too wrong to base a review on.',
    },
    digest_risk_reason: { type: 'string' },
  },
}

// ---------------------------------------------------------------------------
// Prompts
// ---------------------------------------------------------------------------

const auditPrompt = (doc) => `You are auditing ONE documentation file for factual accuracy against the code it claims to describe.

REPO (read-only): ${TARGET}
DOC: ${TARGET}/${doc.path}
WHAT THIS DOC IS FOR: ${doc.role}

Why this matters: a full-repo code review is about to build its "house rules" digest from this doc and inject it into ~90 reviewer agents. Every false claim in here becomes a false premise in dozens of code reviews. You are the filter.

METHOD — verify, do not skim:
1. Read the doc completely.
2. Enumerate its CHECKABLE claims. A checkable claim is one the tree can settle: a path exists, a crate/module/file exists, a symbol exists, a make target exists, a rule is actually followed, a version matches, a subsystem works the way it is described.
3. CHECK EACH ONE. Use Glob for paths, Grep for symbols and rules, Read for the code the doc describes. Do not trust the doc. Do not trust your memory of the repo. Run the check.
4. Prose that is not checkable (rationale, philosophy, "why we did it this way") is NOT drift. Skip it. Do not pad your findings with it.

CLASSIFY each drift you find — this is the most important judgment you make:
- doc-stale: the doc is out of date; the code is fine. Propose the corrected text.
- code-drifted: the rule in the doc is still what the project MEANS, and the CODE has broken it. Do NOT propose a doc edit. This is a code bug, and it gets handed to the code review.
- ambiguous: the doc describes an intent the code never implemented, and deciding who is wrong requires a human. DO NOT GUESS. Rewriting a doc to match reality is correct when reality is correct — and it is whitewashing when the code is the thing that drifted. When you cannot tell, say so.

A worked example of the distinction, from this repo:
- ".agents/project-structure.md says source/ui/ is an in-repo package, but it isn't in the tree — it's an npm dep now" → doc-stale. The move to a published package was deliberate; the doc just didn't keep up.
- "auth.md says EVERY service method opens with require_admin()?, and ServiceX::foo() doesn't" → code-drifted. The rule is load-bearing and still meant. The code is wrong, not the doc.

HONESTY REQUIREMENTS:
- Report verified_claims as the number you ACTUALLY checked. If you checked 6 things, say 6 — do not say 40.
- If the doc is accurate, say so and return an empty drifts array. "Accurate" is a real and valuable result. Do not invent drift to look productive.
- Every drift needs evidence that is a concrete check — the path you globbed, the grep you ran and what it returned, a file:line. "Seems outdated" is not evidence.
- If a claim is TRUE but trivially so, don't report it at all.

Return the structured object.`

const synthPrompt = (reports) => `You are synthesizing a documentation audit. ${reports.length} agents each audited one doc against the real tree.

Their reports:
${JSON.stringify(reports, null, 2)}

Do three things a per-doc agent structurally could not:

1. CROSS-DOC CONTRADICTIONS. Find places where two docs disagree with EACH OTHER, not with the code. Each per-doc agent only saw its own file. Example shape: architecture.md describes a subsystem one way, project-structure.md another. Only report contradictions you can point at concretely.

2. NEEDS-YOUR-CALL. Take every 'ambiguous' drift and restate it as a crisp question a human can answer in one line, each with your recommended answer and the reasoning. This is the section the human actually reads, so make each question decidable — "should the doc be updated to drop the route-verification section, or is that still planned work?" beats "what should we do about route verification?".

3. DIGEST RISK. The next workflow builds a conventions digest from these docs and injects it into ~90 code reviewers. Call it:
   - safe: reviewers can trust these docs as-is.
   - fix-first: some doc-stale items would actively mislead reviewers into filing false findings. Name them.
   - blocked: too wrong to base a review on.
   Judge by BLAST RADIUS, not by drift count. A stale path in project-structure.md is cosmetic. A wrong rule in auth.md or code-conventions.md would generate dozens of bogus convention findings across the whole review — that is fix-first at minimum.

Also pass through the 'code-drifted' items as seed findings for the code review.

Return the structured object.`

const scribePrompt = (reports, synth) => `Write the documentation-accuracy report. You are the only agent in this workflow that writes to disk.

Create the directory ${OUT} if needed, then write exactly one file: ${OUT}/docs-report.md

Per-doc audits:
${JSON.stringify(reports, null, 2)}

Synthesis:
${JSON.stringify(synth, null, 2)}

STRUCTURE the report as follows. It is read by a human deciding what to fix before launching a large code review, so lead with the decision, not the data:

# Documentation accuracy report — ${DATE}
Reviewed: main @ (run \`git -C ${TARGET} rev-parse --short HEAD\` and put the real sha here)

## Verdict
The digest risk (safe / fix-first / blocked) and what it means in one paragraph. If fix-first, name the specific docs to fix before the code review runs.

## Needs your call
The ambiguous items, as a numbered list. Each: the question, why it's ambiguous, your recommendation. This is the section that gets acted on — put it high.

## Fix before the code review
The doc-stale drifts whose blast radius is real, with the proposed corrected text in a fenced block, ready to paste.

## Cosmetic drift
Doc-stale items that would not mislead a reviewer. A compact table: doc | section | says | reality.

## Code drifted from the docs (→ handed to the code review)
Rules the docs still mean and the code has broken. These are code bugs, NOT doc edits.

## Cross-doc contradictions
Where the docs disagree with each other.

## Coverage
Table: doc | verdict | claims verified | drifts found. Then, honestly, what this audit did NOT check.

WRITING RULES:
- Markdown tables for enumerable facts; prose for judgment.
- Quote evidence. Every claim of drift shows the check that proves it.
- Do not editorialize or pad. If a doc is accurate, one row in the coverage table is the whole story.
- Do NOT edit any documentation file. You write one file: the report. Nothing else.

When done, reply with ONLY: the absolute path written, the digest risk verdict, the total drift count, and the needs-your-call count.`

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

phase('Audit')
log(`Auditing ${DOCS.length} docs against ${TARGET}`)

const reports = (
  await parallel(
    DOCS.map((doc) => () =>
      agent(auditPrompt(doc), {
        label: `audit:${doc.path}`,
        phase: 'Audit',
        schema: DRIFT_SCHEMA,
        model: 'sonnet',
      })
    )
  )
).filter(Boolean)

const totalDrift = reports.reduce((n, r) => n + (r.drifts ? r.drifts.length : 0), 0)
const ambiguous = reports.reduce(
  (n, r) => n + (r.drifts || []).filter((d) => d.classification === 'ambiguous').length,
  0
)
log(`${reports.length}/${DOCS.length} docs audited · ${totalDrift} drifts · ${ambiguous} need a human call`)

phase('Synthesize')
const synth = await agent(synthPrompt(reports), {
  label: 'synthesize',
  phase: 'Synthesize',
  schema: SYNTH_SCHEMA,
  model: 'opus',
})

log(`Digest risk: ${synth ? synth.digest_risk : 'unknown'}`)

phase('Write')
const written = await agent(scribePrompt(reports, synth), {
  label: 'scribe',
  phase: 'Write',
  model: 'sonnet',
})

return {
  report: `${OUT}/docs-report.md`,
  docs_audited: reports.length,
  total_drift: totalDrift,
  needs_your_call: synth ? (synth.needs_your_call || []).length : 0,
  digest_risk: synth ? synth.digest_risk : 'unknown',
  digest_risk_reason: synth ? synth.digest_risk_reason : '',
  handoff_to_run_b: synth ? synth.handoff_to_run_b || [] : [],
  scribe: written,
}
