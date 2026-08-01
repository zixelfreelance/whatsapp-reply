# Prior Art Analysis

This project borrows from three existing systems. This document analyzes what each provides
and how we reuse/adapt it.

> **Verified 2026-08-01.** An earlier version of this document oversold the reuse story.
> Claims below have been checked against the actual repos on disk; corrections are marked
> inline. Read the corrections before planning around any of this — the headline finding is
> that the WhatsApp support in `passive-capture` is two lines of hostname matching with no
> message extraction whatsoever.

---

## 1. passive-capture

**Location:** `/Users/bm/zix-freelancer-capture/org/passive-capture`  
**Status:** Production v0.1 released  
**Stack:** Rust backend (Axum, Tokio, SQLite) + TypeScript extension (Chrome MV3)

### What It Provides

**Extension (Chrome MV3)**
- 4-layer capture pipeline: embedded, hydration, network, DOM
- Platform detection: 10+ platforms (YouTube, LinkedIn, Twitter, GitHub, Reddit, Instagram, TikTok, Upwork, Freelancer, **WhatsApp**)
- Content hashing (SHA-256) deduplication
- Service worker batching (every 2s or 5+ items)
- localStorage fallback when service worker reloads (MV3 lifecycle issue)
- Real-time sidepanel UI showing captures as they flow
- License tier gating (Free blocks packs, Pro/Enterprise enables them)

**Backend (Rust)**
- HTTP server on localhost:9999
- SQLite persistence to `~/lake/raw/YYYY-MM-DD/`
- Structured tracing (IngestStart, CaptureReceived, CapturePersisted, etc.)
- Pack registry (YouTube yt-dlp extraction works)
- Queue module with scoring (tier_0_hard_filter, tier_1_rule_score, fuse_rank)
- Judgment client (for external evaluation)
- Projector (transforms captures into queryable database)

**Test Suite**
- 33 tests passing
- Covers capture pipeline, deduplication, tier gating

### How We Reuse It

#### Direct Copy
- **Extension structure:** manifest.json + service worker pattern
- **Service worker batching:** same logic and same tuned values (2s or 5+ items)
- **`trace.rs`:** structured logging, near-verbatim
- **`localStorage` fallback:** the MV3 service-worker-death workaround

#### ⚠️ Correction: the "WhatsApp detector" is not what this document implied

The entirety of WhatsApp support in `passive-capture` is two lines in
`src/extension/sw/ingest.ts`:

```ts
} else if (host.includes('whatsapp.com') || host.includes('web.whatsapp.com')) {
  return 'whatsapp';
}
```

That returns a platform *label* from a hostname. There is **no message extraction, no chat
identification, no sender parsing** — none of what this project actually needs. Reading
messages out of WhatsApp Web's obfuscated React DOM is greenfield work, and it is the
single largest unestimated task in the plan.

#### ⚠️ Correction: the extension toolchain differs from our docs

`passive-capture` is **Bun + WXT + vitest**, and its sidepanel is a plain `.ts` file — not
npm + React `.tsx` as this project's README originally specified. Its `package.json` build
script is a placeholder (`echo 'no frontend bundle step yet'`). Forking it means inheriting
WXT and Bun; writing fresh means not reusing its build at all. **Decide before Week 1 Day 4.**

#### Adapt
- **Backend endpoints:** passive-capture has `/ingest` (receive captures). We add `/action/execute` (send actions) and `/audit` (query trail)
- **SQLite schema:** Add `actions` + `results` tables to track action execution (alongside captures table)
- **Queue module:** Reuse concepts (ActionItem, ActionKind, ActionState) but adapt for action execution vs. capture filtering

#### Skip
- **Packs (yt-dlp):** Not needed for whatsapp-reply (we're not extracting metadata)
- **Judgment client:** Not needed initially (Claude API replaces it)
- **Projector:** Can use it later for querying by platform/date

### Code to Copy

```bash
# Extension architecture
cp -r passive-capture/src/extension/ whatsapp-reply/src/extension/

# Backend patterns
cp -r passive-capture/src/ whatsapp-reply/src/orchestrator/
# Keep: ingest.rs patterns, trace.rs, queue module concepts
# Remove: packs.rs, judgment.rs, projector.rs (will add our own)
```

### Learnings from passive-capture

1. **Extension lifecycle issue:** Service workers reload in MV3. localStorage fallback queues data. We'll do same.
2. **Platform detection:** Simple URL-based detection works well. No need for heuristics.
3. **Batching:** Every 2s or 5+ items is sweet spot. Tested on real platforms.
4. **SQLite:** rusqlite + bundled sqlite3 is production-ready. No external DB needed.
5. **Structured logging:** Using tracing crate + JSON output is powerful for debugging.

---

## 2. zix-agent

**Location:** `/Users/bm/zix-agent`  
**Status:** Active development  
**Stack:** Rust workspace (3 crates) + React frontend

### Project Structure

**Crates:**
- `zix-core` — core logic/models
- `zix-jcode-bridge` — integration with jcode (how to call jcode from Rust)
- `zix-server` — HTTP API server (Axum, Tokio, SQLite)

**Frontend:**
- React + Vite + TypeScript in `src-frontend/`

### What It Provides

**zix-server (HTTP Server)**
- Axum web framework scaffold
- Tokio async runtime
- SQLite database integration
- Error handling patterns
- Config loading (env vars, TOML)
- Handler structure (handlers.rs, connectors.rs, skills.rs, ipc.rs)

**zix-jcode-bridge (jcode Integration)**
- How to call jcode from Rust
- Request/response serialization to jcode
- Multi-model support (Claude, OpenAI, local Ollama)

**zix-core**
- Shared data types
- Trait definitions for extensibility

### How We Reuse It

#### Direct Copy
- **Server scaffold:** Axum + Tokio setup from zix-server/src/main.rs
- **Error handling:** Use same Result types + error propagation
- **Config loading:** Use same pattern (env vars + TOML file)

#### Adapt
- **Handlers:** We'll adapt connectors.rs pattern for agent-browser spawning + Claude API calls
- **Skills:** If we want jcode integration later, use zix-jcode-bridge as reference

#### Skip
- **Frontend:** whatsapp-reply doesn't need React (extension has sidepanel)
- **IPC:** Not needed (everything in one process)

### Code Structure from zix-agent

```rust
// src/orchestrator/src/
├── main.rs                  (from zix-server/src/main.rs pattern)
├── server.rs                (from zix-server/src/server.rs)
├── executor.rs              (new: agent-browser + Claude loop)
├── queue.rs                 (from passive-capture queue module)
├── claude.rs                (new: Claude API client)
└── models.rs                (shared types)
```

### Learnings from zix-agent

1. **Async Rust patterns:** Tokio handles concurrency well. Use spawn_blocking for subprocess (agent-browser).
2. **Config management:** env vars + TOML works. Set CLAUDE_API_KEY env var, read from config.toml for orchestrator settings.
3. **HTTP error handling:** Axum's error types + custom Error enum is clean.
4. **jcode integration:** Pattern exists if we need multi-model later. Current plan: direct Claude API.

---

## 3. jcode

**Location:** `~/jcode` (a separate repo — **not** in this one, as an earlier draft claimed)
**Status:** Production
**Stack:** Rust, 100+ crates
**What it actually is:** a **coding agent harness** — "the next generation coding agent
harness to raise the skill ceiling," by `1jehuang`. It is a peer to Claude Code, not a
generic multi-model LLM proxy.

> ⚠️ **This reframes the integration story below.** Routing WhatsApp reply generation
> through jcode would mean bending a coding-agent harness into a chat-reply service. The
> "Phase 2 integration" notes that follow were written against the wrong mental model and
> should be treated as speculative, not planned.

### What It Provides

**Multi-Model LLM Support**
- Claude (Anthropic)
- ChatGPT (OpenAI)
- Gemini (Google)
- Bedrock (AWS)
- Ollama (local)
- Custom providers

**Agent Framework**
- Agent runtime (executes loops with tool calling)
- Tool registry (30+ tools: write, analyze, post, schedule, etc.)
- Swarm orchestration (multiple agents coordinating)
- Provider abstraction (swap LLMs without code changes)

**Architecture**
- Modular crate structure
- Async-first (Tokio)
- Type-safe tool definitions
- Error handling + retries

### How We Reuse It

#### Phase 1 (MVP): Skip jcode
- Use Claude API directly (simpler, one less dependency)
- Direct `curl` to Claude API via `reqwest` crate

#### Phase 2 (Future): Optional Integration
- If we want multi-model support: call jcode via HTTP
- If we want complex workflows: use jcode's swarm orchestration
- If we want tool calling: leverage jcode's tool framework

#### Keep in Mind
- jcode is complex (30 crates, powerful but heavy)
- For MVP, simpler to call Claude API directly
- Integration point: send jcode a message, get back response
- Use zix-jcode-bridge pattern as reference

### Learnings from jcode

1. **Multi-model abstraction:** Clean trait-based architecture. Can add later without rewriting.
2. **Tool framework:** If we want to give Claude new capabilities (search, database query), jcode's tool system is ready.
3. **Error handling:** jcode's error types + retry logic are worth studying.
4. **Deployment:** jcode runs as standalone service. Same pattern for whatsapp-reply orchestrator.

---

## Integration Timeline

### Week 1-2: Reuse passive-capture + zix-agent

```
whatsapp-reply v0.1 = passive-capture (capture layer)
                    + zix-agent (server scaffold)
                    + agent-browser (new: execution layer)
                    + Claude API (new: reasoning layer)
```

- Extension: fork from passive-capture, add action result display
- Orchestrator: build from zix-agent patterns, add executor + Claude client
- Database: extend passive-capture schema with actions + results tables

### Week 2-3: Optional jcode Integration

```
whatsapp-reply v0.2 = v0.1 + jcode (multi-model, complex workflows)
```

- Only if needed (multi-language replies, sentiment analysis, workflow orchestration)
- Keep v0.1 simple: one model (Claude), one task (reply)
- v0.2 can be multi-tenant, multi-model fork

---

## File Mapping: What Goes Where

| Source | Destination | Adapt? | Keep? |
|--------|-------------|--------|-------|
| passive-capture/src/extension/ | whatsapp-reply/src/extension/ | Adapt manifest, add action result UI | Keep: platform detection, batching, sw lifecycle |
| passive-capture/src/ingest.rs | whatsapp-reply/src/orchestrator/src/server.rs | Adapt routes | Reuse: request parsing, structured logging |
| passive-capture/src/queue.rs | whatsapp-reply/src/orchestrator/src/queue.rs | Adapt for actions | Reuse: SQLite operations, tier logic pattern |
| passive-capture/src/trace.rs | whatsapp-reply/src/orchestrator/src/trace.rs | No change | Keep: structured logging |
| zix-agent/crates/zix-server/src/ | whatsapp-reply/src/orchestrator/src/ | Adapt to our needs | Reuse: Axum patterns, config loading, async patterns |
| zix-jcode-bridge/ | (skip for MVP) | — | Ref only (for Phase 2) |
| jcode/ | (skip for MVP) | — | Ref only (for Phase 2) |

---

## Decisions Made

### 1. Don't Fork passive-capture

**Option A:** Fork passive-capture, add action executor  
**Option B:** New repo, copy/adapt relevant parts

**Chosen:** Option B (new repo)

**Why:**
- passive-capture is capture-only. whatsapp-reply is capture + execute.
- Different deployment target (whatsapp-reply runs locally, passive-capture is multi-tenant)
- Cleaner history if we want to diverge later
- Can merge learnings back to passive-capture if successful

### 2. Direct Claude API, Not jcode

**Option A:** Use jcode for multi-model support  
**Option B:** Call Claude API directly

**Chosen:** Option B (direct API)

**Why:**
- Simpler (fewer dependencies, faster to ship)
- Lower latency (direct HTTP vs. proxy through jcode)
- Cost is same (~$3/month for MVP)
- Roadmap to add jcode later without major refactor (use zix-jcode-bridge pattern)

### 3. agent-browser, Not Playwright

**Option A:** Playwright + manual screenshot parsing  
**Option B:** agent-browser CLI

**Chosen:** Option B (agent-browser)

**Why:**
- Built for AI (semantic refs, accessibility tree)
- Simpler for Claude to reason about
- Rust native (aligns with orchestrator)
- Vercel Labs maintains it (production-grade)

---

## Open Questions

1. ~~**Should we share SQLite schema between passive-capture and whatsapp-reply?**~~
   **ANSWERED: No. Never.** On 2026-07-12 a LinkedIn sensor in this estate wrote private
   DMs into `pastely-app`'s Upwork-only lake — see `pastely-app/CLAUDE.md` ADR 0009. Every
   sensor is a fork of the same brain, which is exactly how that happened. This project
   handles private personal conversations, the most sensitive data of any sibling. Own
   database, own port, own lake, and a hard `platform == "whatsapp"` assert on ingest.
   See [ESTATE_LESSONS.md](ESTATE_LESSONS.md#4-the-cross-contamination-incident--read-this-one-twice).

2. **Should orchestrator integrate with jcode's tool framework?**
   - Pro: Access to 30+ tools (search, analyze, post)
   - Con: Complexity, jcode dependency
   - Current: No. If Claude needs more tools, we'll add them directly.

3. **Should extension use passive-capture's Pack system?**
   - Pro: Already extracts metadata (YouTube yt-dlp)
   - Con: Overkill for MVP, jcode handles analysis
   - Current: No. Packs are for enrichment, we're doing pure response generation.

---

## Reuse Checklist

- [ ] Extension: Clone manifest.json + service worker pattern
- [ ] Extension: Test platform detection on WhatsApp Web
- [ ] Orchestrator: Clone Axum server scaffold from zix-server
- [ ] Orchestrator: Adapt ingest.rs for /action/execute endpoint
- [ ] Orchestrator: Clone SQLite operations from passive-capture/queue.rs
- [ ] Orchestrator: Clone structured logging from passive-capture/trace.rs
- [ ] Config: Use same pattern (env vars + TOML) as zix-agent
- [ ] Test: Write integration test (message → Claude → reply)

---

## References

- **passive-capture:** `/Users/bm/zix-freelancer-capture/org/passive-capture/` ✓ verified
- **zix-agent:** `/Users/bm/zix-agent/` ✓ verified (Tauri v2 shell at `src-tauri/`)
- **jcode:** `/Users/bm/jcode/` ✓ verified (not `./jcode`)
- **agent-browser:** https://github.com/vercel-labs/agent-browser — Rust (2.8 MB Rust vs
  340 KB TS), 39.6k stars, created 2026-01-11, `v0.33.1` at time of writing. Distributed
  via npm as a shim around prebuilt binaries. **Not installed on this machine yet.**
