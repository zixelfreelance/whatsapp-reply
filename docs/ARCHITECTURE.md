# Architecture: whatsapp-reply

## System Overview

**Goal:** Claude-powered WhatsApp reply assistant that captures messages, drafts replies with Claude, gets human approval, and sends via browser automation.

**Design Philosophy:**
- **Human in the loop, always** — nothing sends without a person approving it
- **Account safety is the critical path** — behavioural pacing over technical cleverness
- **Data stays local by default** — two accepted external services: Anthropic and Telegram
- **Cost-effective** — zero platform fees (no WhatsApp Business API), only Claude API usage
- **WhatsApp done properly, seams for the rest** — not a generic engine up front

**Scope:** 1:1 chats only. Groups are out of v0.1 (no deep link, non-unique names, weaker
targeting guarantee, and a misfired group reply is far more costly than a misfired DM).

---

## Three-Layer Architecture

### Layer 1: Capture (Chrome Extension)

**Technology:** TypeScript, Chrome MV3 manifest

**What it does:**
- Detects messages on WhatsApp Web (`MutationObserver`, event-driven — never polls)
- Extracts: chat_id, sender, message text, timestamp
- Batches messages (every 2s or 5+ messages — values inherited from `passive-capture`)
- POSTs to orchestrator
- Draws the activity border while the agent is acting

**This layer is read-only. It never sends.**

**Key components:**
- `manifest.json` — MV3 permissions + sidepanel
- `background.ts` — Service worker, batch logic, `localStorage` fallback
- `content.ts` — DOM capture, message detection, activity border
- `whatsapp.ts` — Platform-specific extractor (built here; prior art has hostname
  detection only, no message extraction)
- `sidepanel` — Local capture/status viewer

**Why this layer:**
- Runs in your browser with your login state — no separate authentication
- Runs in an **isolated world**: the page cannot see it, so capture is undetectable
- Sees the rendered page, not just the network layer

---

### Layer 2: Orchestrator (Rust Backend)

**Technology:** Rust, Tokio async runtime, Axum HTTP server, SQLite

**What it does:**
- Receives captured messages via `POST /action/execute`
- Manages SQLite action queue
- Orchestrates the Claude reasoning loop
- Spawns agent-browser subprocesses
- Logs everything to SQLite + structured JSON logs

**Key components:**
- `main.rs` — Entry point, config loading
- `server.rs` — Axum HTTP routes (`/action/execute`, `/audit`)
- `executor.rs` — Main loop (read queue → Claude → agent-browser → update result)
- `queue.rs` — SQLite schema + queue operations
- `claude.rs` — Claude API client (vision, reasoning)

**SQLite Schema:**
```sql
CREATE TABLE actions (
    id TEXT PRIMARY KEY,
    platform TEXT NOT NULL CHECK (platform = 'whatsapp'),  -- see note below
    content_hash TEXT NOT NULL UNIQUE,  -- sha256(chat_id|sender|text|timestamp)
    chat_id TEXT NOT NULL,           -- expected target; asserted before send
    contact TEXT NOT NULL,           -- display name / number, for the Telegram card
    inbound_text TEXT NOT NULL,      -- what they sent
    draft_text TEXT,                 -- what Claude proposed
    approved_text TEXT,              -- what the human actually approved (may be edited)
    status TEXT NOT NULL,            -- see state machine below
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    drafted_at TIMESTAMP,
    approved_at TIMESTAMP,
    completed_at TIMESTAMP,
    error_msg TEXT
);

CREATE TABLE results (
    id TEXT PRIMARY KEY,
    action_id TEXT NOT NULL,
    success BOOLEAN NOT NULL,
    error_msg TEXT,
    commands JSON,                   -- ["fill @e1 '...'", "press Enter"]
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (action_id) REFERENCES actions(id)
);
```

**Status state machine:**

```
captured → drafted → awaiting_approval ─┬→ approved → executing ─┬→ done
                                        │                        └→ failed
                                        ├→ skipped   (human said no)
                                        └→ expired   (no answer in N hours)
```

> **No `snapshots` table.** An earlier draft persisted every accessibility tree for
> offline replay. That's redundant with the structured log and expensive per action.
> Instead the executor logs the tree **on failure only** — same diagnostic value, none
> of the storage cost. Decision made 2026-08-01.

> **`content_hash UNIQUE` is not optional.** WhatsApp Web re-renders its message list
> constantly (read receipts, presence, reactions, virtualised scroll), so a naive
> `MutationObserver` re-emits the same message many times. Without dedup a duplicate
> capture becomes a duplicate draft, a duplicate Telegram card, and — if approved twice —
> the *same reply sent twice*, which is both embarrassing and bot-shaped. Every capture
> sibling in the estate hashes for this reason.

> **`platform` is asserted, not coerced.** On 2026-07-12 a LinkedIn sensor in this estate
> wrote private DMs into another repo's lake. Every sensor forks the same spine, which is
> how it happened. This project handles the most sensitive data of any sibling, so a
> capture that isn't `whatsapp` is rejected outright. Own DB, own port, own lake, never
> unified. See [ESTATE_LESSONS.md](ESTATE_LESSONS.md).

> **Time is threaded, never stamped mid-pipeline.** The message's real arrival time flows
> from capture through drafting into the record; nothing calls `Date.now()` during
> projection. `capture-reddit` hit the inverse bug — projection-time timestamps broke
> replay determinism and silently duplicated rows. For us it's also load-bearing for
> safety: `min_age_seconds` is meaningless if arrival time is re-stamped.

**Rate Governor**

Sits between `awaiting_approval → approved` and `executing`. Nothing reaches the browser
without passing it:

| Control | Purpose |
|---------|---------|
| `max_per_hour` | Hard ceiling on sends |
| `min_gap_seconds` | Spacing between two consecutive sends |
| `jitter_seconds` | Random 0..N added to every delay, so timing isn't metronomic |
| `min_age_seconds` | Floor on how soon after arrival a reply may go out |

`min_age_seconds` is the one people skip and shouldn't. Instant replies are the clearest
bot signal there is — nobody answers in 900ms at 4am. The governor also means a backlog
(laptop asleep overnight, 40 messages queued) drains over hours rather than firing as a
burst, which is the single most machine-shaped pattern the system could emit.

**Why Rust:**
- Same stack as jcode (reusable patterns)
- Tokio handles concurrent message processing
- SQLite bundled (no external DB)
- Safe, fast, type-safe

---

### Layer 3: Execution (agent-browser)

**Technology:** Rust CLI (Vercel Labs), browser automation

**What it does:**
- Spawned as subprocess by orchestrator
- Takes screenshot of browser
- Parses accessibility tree → semantic refs (`@e1`, `@e2`, etc.)
- Receives commands from Claude (`click @e2`, `type "text"`)
- Executes commands
- Returns result (success/failure)

**Why agent-browser:**
- Built for AI agents (accessibility tree, semantic refs, not CSS selectors)
- Rust-native binary (npm is only the distribution channel)
- Resilient to UI changes (accessibility tree is more stable than CSS)
- **Generates trusted input events** — see Decision 5 below
- Supports the pair-browsing model directly: it streams the viewport for a human to watch
  alongside the agent, and its idle-shutdown explicitly never closes a browser in direct
  human use

**Unverified as of 2026-08-01 — resolve before writing executor code:**
1. Does `--auto-connect` attach to an already-running, logged-in Chrome **without**
   `--remote-debugging-port`? The docs group it with CDP sessions, which normally need one.
2. Can it fill and send in WhatsApp Web at all? The composer is a React/Lexical
   `contenteditable` behind a strict CSP.

Both are load-bearing. If either fails, the read/write split collapses and the extension
must send instead.

---

## Data Flow

### Happy Path: Message → Draft → Approval → Reply

```
1.  Someone messages you in WhatsApp Web
    ↓
2.  Extension content script sees the DOM change (MutationObserver)
    ↓
3.  Extension extracts: chat_id, from, text, timestamp — batches (2s or 5+)
    ↓
4.  POST /capture
    ↓
5.  ALLOWLIST GATE — chat not opted in? recorded, never drafted. Stop.
    ↓
6.  Orchestrator stores: status = "captured"
    ↓
7.  Claude drafts a reply (ONE API call, text only — no page involved yet)
    status = "drafted"
    ↓
8.  Telegram card: contact, their message, the proposed reply
    [✓ Send]  [✎ Edit]  [✗ Skip]
    status = "awaiting_approval"
    ↓
9.  You tap ✓ (or edit then send). status = "approved"
    ↓
10. RATE GOVERNOR — hold until max_per_hour, min_gap, min_age all satisfied
    ↓
11. agent-browser navigates to the target chat (deep link)
    ↓
12. ASSERT — read the conversation header, compare to actions.chat_id
    Mismatch → abort, report to Telegram, DO NOT improvise
    ↓
13. agent-browser snapshot → Claude picks refs → fill + send (atomic)
    ↓
14. Reply sent ✅   status = "done"
```

Two gates and one governor stand between a captured message and a sent reply. All three
default closed.

### Failure Path: Escalate, Don't Improvise

```
Any step fails (composer not found, modal covering it, chat assert mismatch)
  ↓
Retry the mechanical action twice — same target, no new reasoning
  ↓
Still failing?
  ↓
Log the accessibility tree (failure only — this is what replaces the snapshots table)
  ↓
status = "failed", and send a failure report to Telegram:
  "Couldn't send to Ali — composer not found. Open WhatsApp?"
  ↓
STOP. Never reason a way around an obstacle unsupervised.
```

This mirrors the take-control principle below: when the agent is confused, hand it to the
human. An agent improvising past a modal it doesn't understand is exactly how an approved
reply ends up somewhere nobody approved.

### Human Priority

The agent runs 24/7 in your own browser window and is visible while acting — the extension
draws a border on the page. Control follows the pattern established by Claude in Chrome,
Comet, and Atlas/Operator rather than being re-derived:

- **Explicit take-control**, not keystroke detection. You click to take the wheel; implicit
  interrupt-on-typing creates races mid-action.
- **Yield at action boundaries.** `fill` + send is atomic — sub-second, never interrupted
  partway, so there is never approved text orphaned in a composer.
- **Explicit resume.** Once you have control the agent waits; it does not auto-resume on idle.
- **Irreversible actions always stop for confirmation** — which in this system means *send*,
  already covered by Telegram approval.

---

## Integration with Prior Art

### Reused from passive-capture

**Extension Architecture:**
- 4-layer capture pipeline (DOM, hydration, network, embedded)
- Platform detection (WhatsApp, LinkedIn, Instagram, etc.)
- Service worker batching + localStorage fallback
- Sidepanel UI for real-time capture viewing
- License tier gating (Free/Pro/Enterprise)

**Backend Patterns:**
- Axum HTTP server scaffold
- SQLite with structured schema
- Structured logging (tracing crate)
- Action queue (concepts from passive-capture's queue module)

**Why reuse:**
- Already production-tested
- Architecture proven for capture + analysis pipelines
- Don't reinvent the wheel

### Reused from zix-agent

**Server Scaffolding:**
- Axum + Tokio stack
- Async request handling
- Error handling patterns
- Config loading (env vars, TOML)

**jcode Integration:**
- zix-jcode-bridge pattern (how to call jcode from Rust)
- Claude API client patterns (if we switch from direct API to jcode proxy)

### Reused from jcode

**AI Agent Loop:**
- Multi-model LLM support (Claude, ChatGPT, local Ollama)
- Tool calling framework
- Agent orchestration patterns

**For whatsapp-reply:**
- Initially use Claude API directly
- Later: integrate jcode for multi-model + workflow orchestration

---

## Key Decisions

### Decision 1: Browser Automation Over Platform APIs

**Options:**
- A) WhatsApp Business API (official, $0.005/msg)
- B) Browser automation (free, any platform)

**Chosen:** Browser automation (agent-browser)

**Why:**
- Cost: $0 vs $0.005/message
- Scope: Works on WhatsApp + LinkedIn + Instagram + Twitter (same code)
- Control: You own the stack, no vendor lock-in

**Tradeoff:**
- Slower (1-2s per action) vs API (100ms)
- Fragile to UI changes (but accessibility tree is resilient)
- Mitigation: Good error handling + retries

---

### Decision 2: Claude API, Not Local LLM

**Options:**
- A) Claude API (online, $0.003 per action, high quality)
- B) Ollama (local, free, lower quality)
- C) Hybrid (try Claude first, fallback to Ollama)

**Chosen:** Claude API initially

**Why:**
- Quality: Claude understands context better than local 7B models
- Cost: Only $3/month for 1000 messages (negligible)
- Speed: Faster inference than local LLM

**Migration path:**
- If cost becomes issue: implement Hybrid mode (Ollama fallback)
- jcode already supports this (multi-model)

---

### Decision 3: Rust Backend, Not Node.js

**Options:**
- A) Rust (Tokio, Axum, SQLite)
- B) Node.js (Express, Bun)
- C) Python (FastAPI)

**Chosen:** Rust

**Why:**
- Aligns with jcode stack (already Rust)
- Better concurrency (Tokio handles 1000s of concurrent actions)
- Type safety (no runtime surprises)
- Performance (native binary, no runtime overhead)

---

### Decision 4: agent-browser Over Playwright

**Options:**
- A) Playwright (general automation, CSS selectors)
- B) agent-browser (AI-native, semantic refs)
- C) Puppeteer (lightweight, similar to Playwright)

**Chosen:** agent-browser

**Why:**
- Built for AI (accessibility tree, semantic refs)
- Doesn't require Node.js (Rust binary)
- Easier for Claude to reason about (refs instead of CSS)
- Vercel Labs maintains it

**Risk to watch:** the repo is 7 months old with 117 releases shipped. Fast-moving young
dependency on the critical path — pin the version.

---

### Decision 5: Extension Reads, agent-browser Writes

**Why not just have the extension send?** It's already in the page and already parses the
DOM to capture. The answer is the trust boundary on input events:

| | Extension content script | agent-browser (CDP) |
|---|---|---|
| Visible to the page? | **No** — isolated world | Attachment is detectable |
| Events it generates | `isTrusted: false` — page can check | `isTrusted: true` — indistinguishable from a human |

So each layer is used where it's strongest: the extension **reads** invisibly, CDP
**writes** with trusted events. This is not a compromise between two options; it's using
both for what each is actually good at.

---

### Decision 6: Draft-and-Approve, Not Auto-Send

**Chosen:** Claude drafts; a human approves every send.

**Why:** it collapses nearly every ban vector at once. Volume becomes human-paced, timing
is naturally jittered, and a bad reply never reaches anyone. It also produces free eval
data — how often you edit before sending tells you precisely when (and whether) auto-send
would be safe.

**The objection it had to survive** was "you have to be at the machine." A Telegram
approval surface removes it — you approve from your phone. Draft-and-approve therefore
costs no autonomy, which is what made it the right call rather than a cautious one.

---

### Decision 7: Telegram as a Second Accepted Vendor

**Chosen:** Telegram bot carrying **full text**, not IDs-and-previews.

**Why:** approving something you can only half-read turns approval into rubber-stamping —
the exact failure mode human-in-the-loop exists to prevent. A truncated preview would
protect privacy by defeating the control it exists to serve.

**Cost, stated plainly:** every captured message and every proposed reply transits
Telegram's servers. That is a real weakening of the "stays local" property and it was
accepted deliberately, not overlooked. See Security & Privacy below.

**Alternatives considered:** sidepanel-only (strictly local, but desk-only), Tauri
dashboard (most code, still desk-only). Both remain viable as *additional* surfaces later —
the approval **protocol** in the orchestrator is the seam, and extra surfaces are thin
clients over it.

---

## Error Handling Strategy

### Orchestrator Errors

1. **Message capture fails** (extension network error)
   - Extension retries locally (localStorage fallback)
   - Retry every 5 seconds until success

2. **Claude API timeout**
   - Exponential backoff (2s → 4s → 8s → 16s)
   - Max 3 retries
   - If still failing: mark action "failed", alert user

3. **agent-browser fails**
   - Retry the mechanical action twice — same target, no new reasoning
   - If still failing: log the accessibility tree, mark `failed`, **report to Telegram**
   - Never improvise around an obstacle unsupervised

4. **Chat assert mismatch** (navigation didn't land where expected)
   - Abort immediately. Do not attempt to correct course.
   - Report to Telegram; the human decides
   - This is the one failure that must never be "handled" cleverly

5. **Approval expires** (no answer within N hours)
   - Mark `expired`, do not send
   - A stale draft answers a conversation that has moved on

6. **SQLite disk full**
   - Log error, stop accepting new captures
   - Alert user: "Disk full, clean up ~/whatsapp-reply/data/"

### Extension Errors

1. **Service worker reload** (Chrome MV3 lifecycle)
   - localStorage fallback queues captures
   - Retry every 5 seconds
   - Once SW back online, flush queue

2. **Network unreachable**
   - Queue locally, retry on network restore
   - Max queue size: 100 messages (configurable)

---

## Security & Privacy

### What Stays Local

- WhatsApp session: your Chrome instance only
- Captures, drafts, approvals: SQLite file on your machine only
- Logs: JSON files on your disk only
- Secrets: environment variables only — never `config.toml`, never SQLite, never logs

### Two Accepted External Services

This project does **not** claim "no third parties." It claims two, named:

| Service | Sees | Why it's accepted |
|---------|------|-------------------|
| **Anthropic** | Inbound message text + generated draft | Inherent to using Claude for reasoning |
| **Telegram** | Inbound message text + proposed draft | Required for remote approval; a truncated preview would turn approval into rubber-stamping |

Platform APIs (WhatsApp Business) are not used — browser automation instead.

> This section previously read "no third-party integrations." That stopped being true when
> Telegram became the approval surface. A pillar you quietly violate is worse than a
> tradeoff you state. Amended 2026-08-01.

### Account Safety

Automating WhatsApp Web is against WhatsApp's terms; the realistic consequence is a ban of
the **phone number**. The signals that trigger it are overwhelmingly behavioural — volume,
reply speed, unsolicited first contact, and recipient reports — not input-mechanism
fingerprints. The safety budget is therefore spent on behaviour:

- Human approval on every send
- Allowlist, empty by default — opt in explicitly, never opt out
- Rate governor (cap, gap, jitter, minimum age)
- Never initiate contact; only reply to conversations that messaged first
- Navigate-then-assert, so an approved reply cannot land in the wrong chat

WhatsApp has no unsend after 15 minutes. An approved-but-wrong reply is permanent, which is
why the assert in step 12 aborts rather than attempting to recover.

### Audit Trail

- Every capture, draft, approval, send, and failure recorded in SQLite
- Query: `SELECT id, status, chat_id, approved_at FROM actions WHERE created_at > '2026-08-01'`
- Full traceability: capture → draft → who approved and when → execution → result
- Accessibility trees logged on failure only

---

## Performance Targets

**The metric that actually decides success:** *do you approve drafts every day?* The estate's
guardrail is "don't build a data graveyard — a foundation with no daily-used lens is failure,
not progress." If Telegram cards go unanswered for a week, this project has failed regardless
of latency or success rate. Track approvals/day alongside the table below.

| Metric | Target | Current |
|--------|--------|---------|
| Approvals per day (the real one) | > 0, sustained | TBD |
| Message latency (capture → reply) | < 30s | TBD |
| Success rate | > 95% | TBD |
| CPU (idle) | < 5% | TBD |
| Memory | < 200MB | TBD |
| Claude API cost/month | < $5 | TBD |

(Measured during Week 3 stress test)

---

## Testing Strategy

### Unit Tests
- Queue operations (SQLite insert/update/read)
- Claude response parsing
- agent-browser command execution

### Integration Tests
- Full loop: message → Claude → agent-browser → reply
- Error scenarios: network timeout, agent-browser fail, Claude error
- Concurrency: 5+ messages at once

### Stress Test
- 24-hour continuous run
- 10+ messages/hour
- Monitor: success rate, latency, errors

See [ROADMAP.md](ROADMAP.md) for test schedule.

---

## Next Steps

1. **Week 1:** Implement orchestrator + extension capture
2. **Week 2:** Implement Claude + agent-browser loop
3. **Week 3:** Stress test + production hardening

See [ROADMAP.md](ROADMAP.md) for detailed phases.
