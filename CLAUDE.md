# CLAUDE.md — whatsapp-reply

> Read this first. It makes this repo self-orienting **without** any org-level access.
> `~/.claude/CLAUDE.md` (global) still applies on top for stack/workflow/identity rules.

## Mission

**A Claude-powered WhatsApp reply assistant for the solo builder (bm).** Messages are
captured passively from WhatsApp Web, Claude drafts a reply, the draft goes to Telegram for
human approval, and only then does browser automation send it.

**Nothing sends without a human tapping approve.** That is the product, not a safety feature
bolted on — it is what makes the whole thing viable on a real account.

Built for ourselves first; packaged for others later. Everything in v0.1 is single-tenant by
nature (your Chrome, your session, your bot token, local SQLite), and multi-tenant is a
rewrite whenever it happens — so we pay nothing toward it now beyond keeping config out of
code.

## Status — `PRE-BUILD` (design done, Day 0 spike NOT run)

- ✅ Design + decision record complete: `docs/ARCHITECTURE.md`
- ✅ Estate lessons harvested from 7 sibling repos: `docs/ESTATE_LESSONS.md`
- ✅ Rust workspace scaffolded, `cargo check` clean
- ⚠️ **Six of eight orchestrator modules are 5-line stubs.** `Server::run()` is
  `std::future::pending()`. There is no extension directory at all.
- 🚫 **Day 0 spike is BLOCKING and has not run.** Three load-bearing assumptions are
  unverified — see *Gates* below. Do not write executor code until they are answered.

## Locked decisions (do not re-litigate — source of truth is `docs/ARCHITECTURE.md`)

- **Draft-and-approve, never auto-send.** Claude writes; a human approves every send. This
  collapses nearly every ban vector at once and yields free eval data (your edit rate tells
  you if auto-send would ever be safe). (Decision 6)
- **Extension reads, agent-browser writes.** A content script runs in an isolated world the
  page cannot see — ideal for capture — but its events carry `isTrusted: false`, which a page
  can detect. CDP input generates trusted events. Each layer is used where it is actually
  strongest. (Decision 5)
- **Telegram is the approval surface, carrying full text.** A truncated preview would turn
  approval into rubber-stamping — defeating the control it exists to serve. The privacy cost
  is accepted and **named**: two vendors, Anthropic and Telegram. (Decision 7)
- **The LLM execution loop stays.** Claude picks refs at send time rather than running a
  fixed script. Chosen for cross-platform seams, against the recommendation to script it.
- **Navigate-then-assert.** Open the target chat by deep link, confirm the conversation
  header matches `actions.chat_id`, *then* fill. Mismatch aborts — never "corrects course."
- **Failure escalates, never improvises.** Two mechanical retries, then a failure report to
  Telegram. An agent reasoning its way past a modal it doesn't understand is how an approved
  reply ends up somewhere nobody approved.
- **Human priority follows the Claude-in-Chrome / Comet / Atlas pattern** — explicit
  take-control (not keystroke detection), yield at action boundaries, explicit resume.
  Adopted, not re-derived.
- **Scope: 1:1 chats only.** Groups are out of v0.1 — no deep link, non-unique names, weaker
  targeting guarantee, higher social cost when wrong.
- **Own database, own port, own lake. Never shared.** See *Guardrails*.

## Spine invariants (inherited from captur's brain — never break)

1. **Raw is sacred** — a re-capture is a no-op; a schema change is a re-projection, never a
   re-fetch. `content_hash UNIQUE` enforces it.
2. **Collector captures; the backend decides** — the extension yields raw captures only.
   Drafting, the rate governor, and approval all live behind the local lake.
3. **Time is threaded, never stamped mid-pipeline** — the message's real arrival time flows
   from capture through drafting into the record. Nothing calls `Date.now()` during
   projection. `min_age_seconds` is meaningless otherwise.

## Gates — the Day 0 spike (BLOCKING)

Answer all three before writing executor code. Deliverable: `spike/findings.md` plus saved
DOM fixtures (which later become the test fixtures, as `capture-reddit/probe/fixtures/` did).

| # | Question | If it fails |
|---|---|---|
| 1 | **Is an unopened chat's message text in the DOM at all?** | The agent must *open* chats to read them — marking them read and turning passive capture into an active behaviour. Different product. **Cheapest to answer, invalidates the most. Do it first.** |
| 2 | Does `--auto-connect` attach to a running, logged-in Chrome **without** `--remote-debugging-port`? | If the port is required, it exposes full browser control to any local process — a real cost to weigh, not absorb silently. |
| 3 | Can agent-browser fill and send in WhatsApp Web? | The read/write split collapses; the extension must send instead, forfeiting `isTrusted`. |

For #3, model the test on `capture-fiver/docs/live-smoke-react-fill.md` — keep its verdict
logic (act, wait 400ms, assert the value **stuck**), but **not** its mechanism: WhatsApp's
composer is a Lexical-family `contenteditable`, so `HTMLInputElement.prototype`'s value
setter does not exist on it. Try `execCommand('insertText')`, then synthetic
`beforeinput`/`input`, then a synthetic paste.

Assert on **extracted field values** (chat_id, sender, full text) — not on "the observer
fired." An endpoint being captured is not the data being present.

## Guardrails

- **This handles the most sensitive data in the estate** — private personal conversations,
  not public job posts. On 2026-07-12 a LinkedIn sensor wrote private DMs into another
  repo's Upwork-only lake (`pastely-app` ADR 0009), because every sensor forks the same
  spine. We are sensor #12 of that lineage. Own DB, own port, own lake, and
  `platform = 'whatsapp'` as a `CHECK` constraint — not a convention.
- **Account safety is behavioural, not technical.** Volume, reply speed, unsolicited first
  contact, and recipient reports are what get numbers banned — not input-mechanism
  fingerprints. The safety budget goes to the rate governor (cap, gap, jitter, minimum age)
  and the empty-by-default allowlist. **Do not build fingerprint evasion**; it's an arms race
  you lose against a party whose terms you're already crossing, and the boring controls are
  what actually keep the account alive.
- **The allowlist defaults to empty.** Opt in explicitly, never opt out.
- **WhatsApp has no unsend after 15 minutes.** An approved-but-wrong reply is permanent.
  This is why the chat assert aborts rather than attempting recovery.
- **Don't build a data graveyard.** The real success metric is **approvals per day**. If
  Telegram cards go unanswered for a week, this project has failed regardless of latency or
  success rate. Track it alongside the performance table.
- **Automating WhatsApp Web is against WhatsApp's terms.** The realistic consequence is a ban
  of the phone number. The repo is public and says so plainly — see README § Risks &
  Limitations. Keep it that way.

## Branch model & identity

- Identity: **`zixelfreelance`** (`.sid-identity` + `.envrc`). Remotes are **SSH only**.
- Repo: `git@github.com:zixelfreelance/whatsapp-reply.git` — **currently PUBLIC**.
- **Main-flow for now**: single `main` branch, direct commits. The estate's opt-in
  gitflow-lite (`dev` integration + `main` release, via `branch-rules init`) is **not**
  enabled here. Adopt it if this grows past one contributor.
- Worktrees, if needed, live at `.worktrees/<branch>/` and need their own `.sid-identity` +
  `.envrc` — they do not inherit.

## Ports — allocate, don't choose

The estate coexists on one host, so ports are assigned deliberately: `captur` 8799/8787,
`captur-linkedin` 8788, `insta` 8789, `capture-fiver` 8790, `FPIOS` 8810. FPIOS's README
calls its number deliberate for exactly this reason.

**`config.toml` currently says `9999`, which was picked arbitrarily and violates the
convention.** A grep of the estate shows 8784, 8787–8792, 8799, 8810, 8811, 8873 in use (some
hits may be coincidental substrings — verify before claiming a slot). **Allocate a real port
from the band and update `config.toml` when the orchestrator is first written.**

## Sources & siblings (read; do NOT disturb)

| Need | Source |
|---|---|
| Structured logging (`trace.rs`, near-verbatim), queue concepts, MV3 `localStorage` fallback, batching values (2s / 5+) | `../passive-capture` |
| Axum/Tokio scaffold, config loading, error handling | `~/zix-agent` |
| Rust + axum + SQLite ingest spine, port discipline | `../freelancer-platform-intelligence-os` |
| The fill smoke-test shape and its MAIN-world fallback | `../capture-fiver/docs/live-smoke-react-fill.md` |
| Probe-before-plan discipline, fixtures-as-evidence, manual-vs-automated verification split | `../capture-reddit/probe/`, `../capture-reddit/docs/VERIFICATION.md` |
| Spine invariants, the cross-contamination ADR | `../pastely-app/CLAUDE.md` |
| Full synthesis of all of the above | `docs/ESTATE_LESSONS.md` |

**`jcode` (`~/jcode`) is a *coding agent* harness**, not a general LLM proxy. Routing reply
generation through it would be bending it out of shape. Referenced, not used.

## Open questions (unresolved — do not invent answers)

1. **Where does this sit against Pastely?** The estate runs explicit WIP=1, and
   `capture-producthunt` is parked behind a validated sale with the guardrail *"the money is
   Upwork / Pastely."* Nobody has said whether this is parked, parallel, or ahead. **Answer
   before Week 1.**
2. **Extension toolchain.** `passive-capture` is Bun + WXT with a plain `.ts` sidepanel; our
   README specifies pnpm + React `.tsx`. Forking means inheriting WXT. Decide before Week 1
   Day 4.
3. **Rust or Bun for the brain.** Rust was chosen for cheap copy-adapt from `passive-capture`
   — an argument weakened once the WhatsApp extractor turned out to be greenfield.
   `pastely-app` ratified TypeScript-over-Rust after a spike; FPIOS is the counterexample.
   Revisit after Day 0, since the spike is TypeScript anyway.
4. **Rate governor numbers.** `max_per_hour`, `min_gap_seconds`, `jitter_seconds`,
   `min_age_seconds` have placeholder values. Tune against real volume.
5. **Group chats.** Out of v0.1, but if they ever come in, targeting needs a mechanism that
   isn't deep links.
