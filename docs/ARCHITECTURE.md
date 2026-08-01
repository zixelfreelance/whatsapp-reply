# Architecture: whatsapp-reply

## System Overview

**Goal:** Claude-powered WhatsApp auto-reply system that captures messages, reasons about them with Claude AI, and sends replies via browser automation.

**Design Philosophy:**
- **Safety first** — all data stays local, no third-party APIs except Claude
- **Cost-effective** — zero platform fees (no WhatsApp Business API), only Claude API usage
- **Builder-grade** — modular, extensible, production-ready

---

## Three-Layer Architecture

### Layer 1: Capture (Chrome Extension)

**Technology:** TypeScript, Chrome MV3 manifest

**What it does:**
- Detects messages on WhatsApp Web
- Extracts: sender, message text, timestamp
- Batches messages (every 2s or 5+ messages)
- Sends to orchestrator via HTTP

**Key components:**
- `manifest.json` — MV3 permissions + sidepanel
- `background.ts` — Service worker, batch logic
- `content.ts` — DOM capture, message detection
- `whatsapp.ts` — Platform-specific detector
- `sidepanel.tsx` — Real-time capture viewer UI

**Why this layer:**
- Extension runs in user's browser with their login state
- No need for separate authentication (WhatsApp session is already in Chrome)
- Can see the rendered page (not just DOM API)

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
    action TEXT NOT NULL,           -- "send_whatsapp"
    params JSON NOT NULL,            -- { to, message }
    status TEXT NOT NULL,            -- "pending", "executing", "done", "failed"
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMP,
    result JSON
);

CREATE TABLE snapshots (
    id TEXT PRIMARY KEY,
    action_id TEXT NOT NULL,
    dom_json JSON NOT NULL,          -- { refs: [{ref, role, label}] }
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (action_id) REFERENCES actions(id)
);

CREATE TABLE results (
    id TEXT PRIMARY KEY,
    action_id TEXT NOT NULL,
    success BOOLEAN NOT NULL,
    error_msg TEXT,
    claude_response JSON,            -- Claude's decision
    commands JSON,                   -- ["click @e2", "fill @e3 'text'"]
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (action_id) REFERENCES actions(id)
);
```

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
- Rust-native CLI (no Node.js overhead)
- Works with any browser (Chrome, Brave, Chromium)
- Resilient to UI changes (accessibility tree is more stable than CSS)

---

## Data Flow

### Happy Path: Message → Reply

```
1. User sends message in WhatsApp Web
   ↓
2. Extension content script sees DOM change
   ↓
3. Extension extracts: from="+1234", message="Hi"
   ↓
4. Extension batches and sends: POST /action/execute
   {
     "action": "send_whatsapp",
     "to": "+1234",
     "message": "Hi"
   }
   ↓
5. Orchestrator stores in SQLite: actions.status = "pending"
   ↓
6. Executor worker reads pending action
   ↓
7. Executor runs: agent-browser snapshot
   → Gets: { refs: [{ ref: "@e1", role: "textbox", label: "Message input" }] }
   ↓
8. Executor sends to Claude:
   "User sent a message. I need to reply with 'Thanks for reaching out!'"
   + accessibility tree
   ↓
9. Claude responds:
   [
     { action: "fill", ref: "@e1", text: "Thanks for reaching out!" },
     { action: "click", ref: "@e2" }
   ]
   ↓
10. Executor runs each command:
    agent-browser fill @e1 "Thanks for reaching out!"
    agent-browser click @e2
    ↓
11. Message sent in WhatsApp Web ✅
    ↓
12. Orchestrator updates SQLite:
    actions.status = "done"
    results.success = true
```

### Retry Path: If agent-browser Fails

```
Agent-browser click fails (modal overlay detected)
  ↓
Executor catches error
  ↓
Wait 2 seconds
  ↓
Take new snapshot
  ↓
Send to Claude: "Last click failed due to modal. Try again?"
  ↓
Claude responds with new command
  ↓
Retry (max 3 retries per action)
  ↓
If still failing: log error, mark action "failed"
```

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
- Doesn't require Node.js (Rust CLI)
- Easier for Claude to reason about (refs instead of CSS)
- Vercel Labs maintains it (production-grade)

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
   - Take new snapshot (UI may have changed)
   - Send to Claude with "last action failed, retry?"
   - Max 3 retries per action
   - If still failing: log error, move to next action

4. **SQLite disk full**
   - Log error, stop accepting new actions
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

### Data Stays Local

- WhatsApp session: your Chrome instance only
- Messages: SQLite file on your machine only
- Logs: JSON files on your disk only
- Claude API: message + claude response (can't be avoided, inherent to using Claude)

### No Third-Party Integrations

- ✅ Local: extension, orchestrator, agent-browser, SQLite
- ❌ Cloud: Claude API (necessary for reasoning)
- ❌ Platform APIs: not used (browser automation instead)

### Audit Trail

- Every action logged to SQLite
- Query: `SELECT * FROM actions WHERE action = 'send_whatsapp' AND created_at > '2026-08-01'`
- Full traceability: capture → decision → execution

---

## Performance Targets

| Metric | Target | Current |
|--------|--------|---------|
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
