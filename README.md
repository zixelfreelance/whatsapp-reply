# whatsapp-reply

**Claude-powered WhatsApp reply assistant** — capture messages, draft replies with Claude, approve from your phone, send via browser automation.

Nothing is sent without a human tapping approve.

## What It Does

```
WhatsApp message arrives
        ↓
Extension captures it (read-only, in-page)
        ↓
Orchestrator queues it  ── rate governor paces everything ──┐
        ↓                                                    │
Claude drafts a reply                                        │
        ↓                                                    │
Telegram shows you the draft  →  [✓ Send] [✎ Edit] [✗ Skip]  │
        ↓ (approved)                                         │
agent-browser navigates to the chat, asserts it landed,      │
fills the composer, sends                              ──────┘
        ↓
Reply sent ✅   (or a failure report back to Telegram)
```

## Quick Start

### Prerequisites
- Node.js 18+ (for extension build)
- Rust 1.70+ (for orchestrator)
- Chrome (for agent-browser)
- Claude API key
- WhatsApp Web (logged in)

### Install

```bash
# Clone
git clone git@github.com:zixelfreelance/whatsapp-reply.git
cd whatsapp-reply
direnv allow .

# Install agent-browser (Rust binary, npm is just the distribution channel)
pnpm add -g agent-browser
agent-browser install

# Build extension
cd src/extension
pnpm install
pnpm build
# Load dist/ in chrome://extensions

# Start orchestrator
cd ../..
cargo run --release
```

### Run

1. Open Chrome → `chrome://extensions` → Load unpacked `src/extension/dist/`
2. Start backend: `cargo run --release`
3. Add at least one chat to `[allowlist]` in `config.toml` — it is empty by default and nothing is drafted until you opt a conversation in
4. Open WhatsApp Web → have someone message you
5. Extension captures it → Claude drafts a reply → the draft appears in Telegram
6. Tap ✓ → agent-browser sends it. Tap ✗ → nothing happens.

## Architecture

**Three layers, plus a human:**

1. **Capture** (Extension) — Chrome MV3 extension. Detects WhatsApp messages, batches, POSTs to the backend. **Read-only** — it never sends.
2. **Orchestrator** (Rust) — HTTP API, action queue, Claude client, Telegram bot, rate governor. Long-lived, because an MV3 service worker isn't.
3. **Execution** (agent-browser) — Navigates to the target chat, snapshots the accessibility tree, executes fill/click on semantic refs.

**Why the split.** A content script runs in an isolated world the page cannot see — ideal for capture. But events it dispatches carry `isTrusted: false`, which a page can detect. CDP-driven input generates trusted events. So each layer is used where it is actually strongest: the extension reads, agent-browser writes. See [ARCHITECTURE.md](docs/ARCHITECTURE.md#decision-5-extension-reads-agent-browser-writes).

**Stack:**
- Rust: Tokio async runtime, Axum HTTP server, SQLite persistence
- TypeScript: Chrome MV3 extension (toolchain TBD — see note below)
- Claude API: reasoning + reply drafting
- agent-browser: browser automation CLI (Rust binary, distributed via npm)
- Telegram Bot API: remote approve/edit surface

> **Toolchain note.** The prior-art extension in `passive-capture` is built with
> WXT + Bun and a plain `.ts` sidepanel — not npm + React. The build commands
> below assume a fresh pnpm setup; if we fork `passive-capture`'s extension
> instead, they change. Decide before Week 1 Day 4.

## Project Structure

```
whatsapp-reply/
├── src/
│   ├── orchestrator/          # Rust backend (HTTP API + action executor)
│   │   ├── src/
│   │   │   ├── main.rs        # Entry point
│   │   │   ├── server.rs      # Axum HTTP routes
│   │   │   ├── executor.rs    # Agent-browser + Claude loop
│   │   │   ├── queue.rs       # SQLite action queue
│   │   │   └── claude.rs      # Claude API client
│   │   └── Cargo.toml
│   │
│   └── extension/             # Chrome extension (TypeScript)
│       ├── manifest.json
│       ├── src/
│       │   ├── background.ts  # Service worker
│       │   ├── sidepanel.tsx  # Real-time UI
│       │   ├── content.ts     # DOM capture
│       │   ├── whatsapp.ts    # WhatsApp detector
│       │   └── utils.ts
│       ├── dist/              # Built output
│       └── package.json
│
├── docs/
│   ├── ARCHITECTURE.md        # System design + decision record
│   ├── PRIOR_ART.md           # Analysis of passive-capture, zix-agent, jcode
│   ├── ROADMAP.md             # 3-week implementation plan
│   ├── API.md                 # (planned — Week 3) Orchestrator endpoints
│   └── DEPLOYMENT.md          # (planned — Week 3) Production checklist
│
├── data/
│   └── whatsapp-reply.db      # SQLite (messages + actions)
│
├── logs/
│   └── orchestrator-*.log     # Structured logs
│
└── Cargo.toml                 # Rust workspace
```

## Configuration

Create `config.toml`:

```toml
[orchestrator]
port = 8812
db = "data/whatsapp-reply.db"
log_level = "info"

[claude]
api_key = "${CLAUDE_API_KEY}"   # env only — never written to this file
model = "claude-sonnet-5"       # workhorse; claude-opus-5 for hard cases

[telegram]
bot_token = "${TELEGRAM_BOT_TOKEN}"   # env only
chat_id = "${TELEGRAM_CHAT_ID}"       # where approvals are sent

# Which conversations may be auto-drafted. DEFAULTS TO EMPTY — opt in
# explicitly, never opt out. A chat not listed here is captured and shown,
# but never answered.
[allowlist]
chats = []

# Behavioural pacing. This is the account-safety control that actually
# matters — see docs/ARCHITECTURE.md § Rate Governor.
[rate]
max_per_hour = 10        # hard ceiling on sends
min_gap_seconds = 90     # minimum spacing between two sends
jitter_seconds = 45      # random 0..N added to every delay
min_age_seconds = 60     # never reply faster than this after a message lands

[agent_browser]
timeout_ms = 10000
max_retries = 3
```

Secrets come from the environment only — never from `config.toml`:

```bash
export CLAUDE_API_KEY=sk-ant-...
export TELEGRAM_BOT_TOKEN=...
export TELEGRAM_CHAT_ID=...
cargo run --release
```

## API

Bound to `127.0.0.1` only. Full reference lands in `docs/API.md` (Week 3).

### POST /capture

Called by the extension when messages arrive. Captures are **not** send instructions —
they enter the queue and a draft is proposed. Nothing sends without approval.

```bash
curl -X POST http://127.0.0.1:8812/capture \
  -H "Content-Type: application/json" \
  -d '{
    "messages": [
      {
        "chat_id": "98912xxxxxxx@c.us",
        "from": "+98912xxxxxxx",
        "text": "can you do Friday?",
        "timestamp": "2026-08-01T03:00:00Z"
      }
    ]
  }'
```

Response:

```json
{ "accepted": 1, "queued": 1, "duplicates": 0 }
```

`duplicates` is expected traffic, not an error — WhatsApp Web re-renders constantly, so the
same message is observed many times. `skipped_not_allowlisted` joins this response when the
allowlist gate lands.

### GET /audit?date=2026-08-01

Query the audit trail — captures, drafts, approvals, sends, failures.

```bash
curl "http://127.0.0.1:8812/audit?date=2026-08-01"
```

### GET /health

Liveness plus dependency status (SQLite, Claude API, agent-browser, Telegram).

### POST /control/pause · POST /control/resume

Kill switch. `pause` stops the executor without losing the queue — the one command to
reach for when something looks wrong.

## Development

### Build Extension

```bash
cd src/extension
pnpm install
pnpm build         # TypeScript → dist/
pnpm watch         # Auto-rebuild on changes
```

### Build Orchestrator

```bash
cargo build --release
cargo test
cargo run --release
```

### Logs

Extension logs: Chrome DevTools Console (F12)  
Orchestrator logs: `tail -f logs/orchestrator-*.log`  
SQLite: `sqlite3 data/whatsapp-reply.db`

## Testing

### Unit Tests

```bash
cargo test
```

### Integration Test (Manual)

1. Start orchestrator: `cargo run --release`
2. Load extension in Chrome
3. Add one test conversation to `[allowlist]`
4. Open WhatsApp Web, have someone message that chat
5. Sidepanel → message captured
6. Telegram → draft arrives (after `min_age_seconds`, not instantly — that's the governor working)
7. Tap ✗ first — confirm nothing sends
8. Repeat, tap ✓ — confirm the reply lands **in the right chat**
9. Verify: `sqlite3 data/whatsapp-reply.db "SELECT id,status,chat_id FROM actions LIMIT 5;"`

### 24-Hour Stability Test

See [ROADMAP.md](docs/ROADMAP.md#week-3-hardening--production-readiness).

## Troubleshooting

**Extension not capturing?**
- Check `chrome://extensions` → extension enabled
- Check console for errors (F12)
- Verify orchestrator running: `curl http://127.0.0.1:8812/health`

**No draft arriving in Telegram?**
- Is the chat in `[allowlist]`? It defaults to **empty** — this is the most likely cause
- Is the governor holding it? `min_age_seconds` and `min_gap_seconds` delay by design
- Check `CLAUDE_API_KEY` set: `echo $CLAUDE_API_KEY`
- Check logs: `tail -f logs/orchestrator-*.log | grep -E 'claude|governor|allowlist'`

**Approved but nothing sent?**
- Expect a failure report in Telegram — the executor never fails silently
- Check the assert: did navigation land on the expected chat? `grep chat_mismatch logs/*`
- Check timeout: increase `timeout_ms` in config

**agent-browser failing?**
- Check installed: `which agent-browser`
- Check Chrome: `agent-browser open https://web.whatsapp.com`
- Check it can see your session — if it opens a QR code, the profile/attach is wrong

## Prior Art

This project borrows **patterns** from three existing systems — mostly not code:

- **passive-capture** — `trace.rs` (structured logging, near-verbatim), queue concepts,
  the MV3 `localStorage`-fallback pattern, and the empirically-tuned batching trigger
  (2s / 5+ items). Note: its "WhatsApp detector" is two lines of hostname matching with
  **no message extraction** — that part is built from scratch here.
- **zix-agent** — Axum/Tokio server scaffold, config loading, error handling.
- **jcode** — a *coding agent* harness, not a general LLM proxy. Referenced, not used.

See [PRIOR_ART.md](docs/PRIOR_ART.md) for the honest per-file breakdown.

## Roadmap

**3-week sprint to MVP:**

- **Week 1:** Orchestrator HTTP API + extension message capture
- **Week 2:** End-to-end loop (message → Claude → reply sent)
- **Week 3:** 24-hour stability test + multi-platform support

See [ROADMAP.md](docs/ROADMAP.md) for detailed phases.

## Cost

- **Claude API:** not yet measured. The old "$0.003/action" figure assumed roughly one
  API call per reply. The real loop is drafting **plus** an accessibility-tree snapshot
  at execution time, and the tree is not small. Expect several times that until prompt
  caching is in and measured. Re-derive during Week 2 Day 13.
- **Infrastructure:** zero (runs locally)
- **Time:** 3 weeks

## Risks & Limitations

Read this before running it against an account you care about.

**Terms of service.** Automating WhatsApp Web is against WhatsApp's terms. The realistic
consequence is a temporary or permanent ban of the **phone number**, not a legal one. This
project exists anyway, but it should not pretend otherwise.

**What actually gets accounts banned** is behavioural, not technical: high volume, replies
faster than a human could type, messaging people who never messaged you, and — above all —
recipients hitting *Report*. Fingerprint-level concerns are a distant second. That is why
the rate governor and the empty-by-default allowlist are load-bearing safety features
rather than nice-to-haves, and why every send passes through a human.

**Mitigations built in:** human approval on every send, allowlist defaults to empty, hourly
cap, minimum gap between sends, timing jitter, a floor on how quickly a reply may follow the
message, no first-contact messages, and navigate-then-assert so an approved reply cannot
land in the wrong conversation.

**Not mitigated:** WhatsApp has no unsend after 15 minutes. An approved-but-wrong reply is
permanent.

**Privacy.** Two external services see your data by design: **Anthropic** (message text and
draft, inherent to using Claude) and **Telegram** (message text and draft, in the approval
message). Everything else — captures, logs, database — stays on disk. See
[ARCHITECTURE.md](docs/ARCHITECTURE.md#security--privacy).

**Scope.** 1:1 chats only. Group chats are out of v0.1 — they have no deep link, their names
are not unique, and the targeting guarantee is materially weaker.

## License

MIT

## Contact

GitHub: [@zixelfreelance](https://github.com/zixelfreelance)

---

**Ship it.** 🚀
