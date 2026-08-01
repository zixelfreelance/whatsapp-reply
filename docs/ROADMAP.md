# Roadmap: 3-Week Implementation

**Goal:** Ship a working Claude-powered WhatsApp auto-reply system.

**Timeline:** 3 weeks (21 days), starting Monday.

**Success Criteria:**
- Week 1: Components work independently
- Week 2: End-to-end loop (message → Claude → reply sent)
- Week 3: 24-hour stability test, production-ready

---

## Day 0: Spike the Execution Layer (BLOCKING)

**Do this before writing another line of orchestrator code.** Two load-bearing assumptions
are unverified, and either one can invalidate the architecture.

> **Read [ESTATE_LESSONS.md](ESTATE_LESSONS.md) first.** Seven sibling repos have already
> run this play. `capture-reddit` lost an entire adapter design to an unprobed assumption;
> `capture-fiver` built the exact smoke-test shape this spike needs.

**Tasks:**
- [ ] **Is unopened-chat message text in the DOM at all?** Open WhatsApp Web, have someone
      message a chat you do *not* have open, and check whether the full text is reachable
      from the DOM or only a truncated sidebar preview. **This invalidates more than the
      agent-browser questions and costs less to answer — do it first.** If the text isn't
      there, the agent must *open* chats to read them, which marks them read and turns
      passive capture into an active behaviour.
- [ ] Install: `pnpm add -g agent-browser && agent-browser install`
- [ ] Verify `--auto-connect` attaches to your **already-running, logged-in** Chrome without
      `--remote-debugging-port`. (The docs group it with CDP sessions, which normally need
      the port. If it *does* need the port, note that it exposes full browser control to any
      local process — a real security cost that must be weighed, not absorbed silently.)
- [ ] Verify agent-browser can `fill` and send in WhatsApp Web at all. The composer is a
      React/Lexical `contenteditable` behind a strict CSP.
- [ ] Send exactly one real message to yourself, end to end, by hand.
- [ ] Write a `contenteditable` fill smoke test, modelled on
      `capture-fiver/docs/live-smoke-react-fill.md`. Keep its verdict logic — act, wait
      400ms, assert the value *stuck* — but swap the mechanism: the composer is a
      Lexical-family `contenteditable`, so `HTMLInputElement.prototype`'s value setter does
      not apply. Try `execCommand('insertText')`, then synthetic `beforeinput`/`input`, then
      a synthetic paste.
- [ ] Assert on **extracted field values** (chat_id, sender, full text), not on "the
      observer fired." An endpoint being captured is not the data being present.

**Deliverable:** `spike/findings.md` — a written yes/no per question, saved DOM fixtures as
evidence, and an explicit recommendation with a named gate. Fixtures become the test
fixtures later, exactly as `capture-reddit/probe/fixtures/` did.

**If either fails:** the "extension reads, agent-browser writes" split collapses and the
extension must send instead — which forfeits the `isTrusted` advantage. Better to learn
this on day zero than day nine.

**Duration:** half a day
**Blocker:** none — nothing else should start until this is answered

---

## Week 1: Foundation (Capture + Orchestrator)

**Goal:** Build the capture layer (extension) and orchestrator HTTP API.

### Day 1-2: Setup + Orchestrator Scaffold

**Tasks:**
- [ ] Set up Rust workspace: `Cargo.toml` + `src/orchestrator/`
- [ ] Create `src/orchestrator/src/main.rs` — HTTP server on :8812
- [ ] Create `src/orchestrator/src/server.rs` — Axum routes (POST /action/execute, GET /audit)
- [ ] Create `src/orchestrator/src/models.rs` — Action, Snapshot, Result types
- [ ] Test: `cargo run --release` starts server, responds to health check

**Deliverables:**
- Orchestrator compiles and runs
- Server listens on http://127.0.0.1:8812
- Can curl: `curl http://127.0.0.1:8812/health`

**Duration:** 2 days  
**Blocker:** None

### Day 2-3: SQLite + Queue

**Tasks:**
- [ ] Create `src/orchestrator/src/queue.rs` — SQLite schema (actions, snapshots, results)
- [ ] Implement queue operations: insert_action, get_pending_action, update_action_status
- [ ] Create database connection pool
- [ ] Write tests: insert → read → update

**Deliverables:**
- SQLite file created at `data/whatsapp-reply.db`
- Schema: actions, snapshots, results tables
- CRUD operations tested
- `cargo test` passes

**Duration:** 1.5 days  
**Blocker:** None

### Day 4: Extension Capture

**Tasks:**
- [ ] Fork extension from passive-capture
- [ ] Update `manifest.json` for whatsapp-reply
- [ ] Implement WhatsApp message detector in `content.ts`
- [ ] Add batch + send logic to `background.ts`
- [ ] Test: load in Chrome, verify messages captured

**Deliverables:**
- Extension builds: `npm run build` in `src/extension/`
- Dist folder contains all files
- Can load in `chrome://extensions` (developer mode)
- Messages appear in sidepanel

**Duration:** 2 days  
**Blocker:** Chrome MV3 manifest syntax

### Day 4-5: Extension → Orchestrator Integration

**Tasks:**
- [ ] Add batch sending to orchestrator: `POST /action/execute`
- [ ] Extension sends: `{ action: "send_whatsapp", to: "+1234", message: "Hi" }`
- [ ] Orchestrator stores in SQLite
- [ ] Test: send message via extension, verify it arrives in DB

**Deliverables:**
- Full loop: extension captures → orchestrator receives → SQLite stores
- `sqlite3 data/whatsapp-reply.db "SELECT * FROM actions;"`
- Data populates

**Duration:** 1 day  
**Blocker:** CORS headers (if extension can't reach :8812)

### Day 5: Logging + Monitoring

**Tasks:**
- [ ] Add structured logging (tracing crate)
- [ ] Implement `src/orchestrator/src/trace.rs`
- [ ] Log every action: received, stored, processing, completed
- [ ] Create `logs/` directory, write JSON logs
- [ ] Dashboard: query logs to see flow

**Deliverables:**
- `logs/orchestrator-*.log` files created
- Each action logged with timestamp + status
- Can tail logs: `tail -f logs/orchestrator-*.log`

**Duration:** 1 day  
**Blocker:** None

### Week 1 Verification

- [ ] Orchestrator runs: `cargo run --release`
- [ ] Extension loads in Chrome
- [ ] Message captured → Orchestrator receives → SQLite stores
- [ ] Logs show full flow
- [ ] Zero compilation errors

**Success Metric:** Components work independently. Ready for integration.

---

## Week 2: Integration (Claude + agent-browser Loop)

**Goal:** End-to-end loop: capture message → Claude analyzes → agent-browser sends reply.

### Day 1-2: Claude API Integration

**Tasks:**
- [ ] Create `src/orchestrator/src/claude.rs` — Claude API client
- [ ] Implement: send snapshot + instruction to Claude
- [ ] Parse response: list of actions to execute
- [ ] Error handling: retries, timeouts, rate limits
- [ ] Test: Claude sees accessibility tree, suggests commands

**Deliverables:**
- `claude.rs` compiles
- Can send snapshot to Claude: `POST https://api.anthropic.com/v1/messages`
- Get back: `["fill @e1 'text'", "click @e2"]`
- Tests pass

**Duration:** 2 days  
**Blocker:** Claude API key (set CLAUDE_API_KEY env var)

### Day 2-3: agent-browser Integration

**Tasks:**
- [ ] Create `src/orchestrator/src/executor.rs` — Main action loop
- [ ] Spawn agent-browser subprocess: `agent-browser snapshot`
- [ ] Parse accessibility tree JSON
- [ ] Send to Claude with instruction
- [ ] Receive commands, execute each
- [ ] Error handling: retry on failure, timeout after 10s

**Deliverables:**
- `executor.rs` compiles
- Can spawn agent-browser, parse output
- Can execute commands: `agent-browser click @e2`
- Tests pass

**Duration:** 2 days  
**Blocker:** agent-browser installed globally

### Day 4: Orchestrator Executor Loop

**Tasks:**
- [ ] Wire executor into main server loop
- [ ] When action arrives: trigger executor worker
- [ ] Executor reads from queue: `get_pending_action()`
- [ ] Takes snapshot, sends to Claude, executes commands
- [ ] Updates status: pending → executing → done (or failed)
- [ ] Test: end-to-end (message → Claude → reply sent)

**Deliverables:**
- Full loop working
- Message sent to WhatsApp via orchestrator
- Verified in browser: reply appears
- SQLite shows: action status = "done"

**Duration:** 1.5 days  
**Blocker:** None

### Day 4-5: Error Handling + Retries

**Tasks:**
- [ ] Handle agent-browser failures: retry with new snapshot
- [ ] Handle Claude timeouts: exponential backoff
- [ ] Handle network errors: queue retry
- [ ] Log errors: what failed, why, when to retry
- [ ] Test: inject failures, verify retries work

**Deliverables:**
- Error logs clear
- Retries work (max 3 per action)
- Failed actions marked in SQLite
- User alerted if action permanently fails

**Duration:** 1.5 days  
**Blocker:** None

### Day 5: Performance Baseline

**Tasks:**
- [ ] Measure latency: message arrival → reply sent (target: < 30s)
- [ ] Measure Claude API cost per action (target: < $0.01)
- [ ] Measure agent-browser timeout scenarios
- [ ] Document performance: latency histogram, error rates

**Deliverables:**
- Performance report: mean/median/p95 latency
- Cost estimate: messages/month → dollars/month
- Baseline for Week 3 stress test

**Duration:** 1 day  
**Blocker:** None

### Week 2 Verification

- [ ] Orchestrator + extension + Claude + agent-browser all integrated
- [ ] Send test message → Claude replies → message sent to WhatsApp
- [ ] No crashes for 1 hour
- [ ] All errors logged, retries working
- [ ] SQLite audit trail complete

**Success Metric:** End-to-end loop works. Ready for stress test.

---

## Week 3: Hardening + Production Readiness

**Goal:** 24-hour stability test, performance optimization, production checklist.

### Day 1-2: 24-Hour Stability Test

**Tasks:**
- [ ] Send 100 test messages over 24 hours (mix of short/long)
- [ ] Monitor: success rate, latency, errors
- [ ] Metrics to track:
  - [ ] Messages captured (target: 100)
  - [ ] Messages replied (target: >95)
  - [ ] Average latency (target: <30s)
  - [ ] Errors (target: <5)
  - [ ] CPU/memory (target: <10% / <200MB)
- [ ] Log everything to SQLite + JSON
- [ ] Generate report

**Deliverables:**
- 24-hour test completed
- Report: captures table, success rate, error logs
- All metrics met

**Duration:** 2 days (1 day setup + monitoring, 1 day next)  
**Blocker:** None (test runs in background)

### Day 2-3: Performance Tuning

**Tasks:**
- [ ] Analyze latency bottleneck: is it Claude (network latency) or agent-browser (snapshot)?
- [ ] Optimize: trim the accessibility tree before it reaches Claude (biggest token cost)
- [ ] Optimize: prompt-cache the system prompt + preamble; measure the hit rate
- [ ] Measure: did optimization help?

> **Do NOT parallelize browser actions.** An earlier draft proposed "parallel snapshots
> (2-3 concurrent actions)." Two agents driving the same WhatsApp Web UI will interleave
> keystrokes into the same composer. v0.1 executes strictly one action at a time. Latency
> is also not worth optimizing hard here — the rate governor deliberately introduces far
> more delay than the pipeline does.

**Deliverables:**
- Latency improved (or documented as network-limited)
- Parallel execution verified
- Baseline vs. optimized comparison

**Duration:** 2 days  
**Blocker:** None

### Day 3-4: Documentation + Deployment Checklist

**Tasks:**
- [ ] **Verify and refresh** README.md and ARCHITECTURE.md against what was actually built
      (both were written up front — this slot is for reconciling them with reality, not
      authoring them)
- [ ] Write API.md — generated from the actual handlers, not from memory
- [ ] Write DEPLOYMENT.md (production checklist, monitoring)
- [ ] Create templates:
  - [ ] config.toml.example
  - [ ] .env.example
  - [ ] launchd plist (macOS auto-start; systemd if Linux ever matters)
- [ ] Document known limitations
- [ ] Document next steps (Week 4+)

**Deliverables:**
- Clean-clone walkthrough: a stranger can follow the README to a working reply
- Architecture document matches the shipped system, decision record current
- Deployment guide: step-by-step to production

**Duration:** 1.5 days  
**Blocker:** None

### Day 4-5: Production Hardening

**Tasks:**
- [ ] Add health check: `/health` returns orchestrator + agent-browser + Claude API status
- [ ] Add monitoring: endpoint that shows live metrics (messages/hour, error rate, latency)
- [ ] Add backup: daily SQLite snapshots
- [ ] Add log rotation: logs don't fill disk
- [ ] Security review: no secrets in logs, no SQL injection, auth if exposed
- [ ] Test: orchestrator survives restart, resumes pending actions

**Deliverables:**
- `GET /health` works
- `GET /metrics` shows live data
- Backups created daily
- Logs rotated (max 100MB per file)
- Restart test passed

**Duration:** 1.5 days  
**Blocker:** None

### Day 5: Release + Handoff

**Tasks:**
- [ ] Create GitHub release (tag: v0.1-beta)
- [ ] Push all code + docs
- [ ] Write release notes: what works, what doesn't, what's next
- [ ] Final verification: all tests pass
- [ ] Record demo video: send message → reply (30s)

**Deliverables:**
- v0.1-beta released on GitHub
- All code committed + pushed
- Release notes complete
- Demo video (optional, but nice to have)

**Duration:** 1 day  
**Blocker:** None

### Week 3 Verification

- [ ] 24-hour test: >95% success rate
- [ ] Latency: <30s average
- [ ] Errors: <5 total
- [ ] CPU/memory: within targets
- [ ] Documentation: complete + tested
- [ ] Production checklist: all items done

**Success Metric:** System is stable, documented, and ready for production.

---

## Success Metrics by Week

| Week | Metric | Target | Status |
|------|--------|--------|--------|
| 1 | Components work independently | ✓ | TBD |
| 1 | Extension captures messages | ✓ | TBD |
| 1 | Orchestrator receives + stores | ✓ | TBD |
| 2 | End-to-end loop | ✓ | TBD |
| 2 | No crashes for 1 hour | ✓ | TBD |
| 2 | Latency < 30s | ✓ | TBD |
| 3 | 24-hour stability | >95% success | TBD |
| 3 | Documentation complete | ✓ | TBD |
| 3 | Production-ready | ✓ | TBD |

---

## Daily Standup Template

```markdown
## Day N Summary

**What was done:**
- [ ] Feature 1 completed
- [ ] Feature 2 completed

**What's next:**
- [ ] Feature 3 (day N+1)
- [ ] Feature 4 (day N+1)

**Blockers:**
- None / [blocker description]

**Time spent:**
- X hours on feature 1
- Y hours on feature 2

**Commits:**
- commit-sha1: "feat: ..."
- commit-sha2: "fix: ..."
```

---

## Deployment Checklist (Week 3 Day 5)

### Pre-Launch
- [ ] Code reviewed + tests pass
- [ ] All dependencies installed
- [ ] Config files created (config.toml, .env)
- [ ] SQLite database initialized
- [ ] Logs directory created

### Launch
- [ ] Start orchestrator: `cargo run --release`
- [ ] Load extension in Chrome
- [ ] Send test message
- [ ] Verify reply sent within 30s

### Post-Launch Monitoring
- [ ] CPU < 10% idle
- [ ] Memory < 200MB
- [ ] Disk usage < 1GB
- [ ] No critical errors in logs
- [ ] All alerts configured

### Rollback Plan
- [ ] Previous version in git tag (v0.1-alpha if needed)
- [ ] Database backup before launch
- [ ] Can revert extension from Chrome settings

---

## Cost Estimate

| Item | Unit Cost | Monthly (1000 msgs) | Notes |
|------|-----------|-------------------|-------|
| Claude API | **TBD — measure in Week 2 Day 5** | TBD | See note below |
| Infrastructure | $0 | $0 | Runs locally |
| Storage (SQLite) | $0 | $0 | ~10MB/month |

> **The old "$0.003/action → ~$3/month" figure was not derived, it was assumed**, and it
> assumed roughly one API call per reply. The real loop is a drafting call **plus** an
> accessibility-tree snapshot at execution time, and trees are not small. Expect several
> times that until prompt caching is implemented and measured. Fill this table with
> measured numbers in Week 2 Day 5 rather than carrying the guess forward.

---

## Next Steps (Week 4+)

### v0.2: Multi-Platform
- [ ] LinkedIn DM replies
- [ ] Instagram DM replies
- [ ] Twitter mentions (read-only platform)

### v0.3: Advanced Features
- [ ] Multi-language replies (Claude can translate)
- [ ] Sentiment analysis (before replying)
- [ ] Custom response templates
- [ ] Integration with jcode (multi-model, workflow orchestration)

### v0.4: Production Hardening
- [ ] Cloud deployment (AWS Lambda / Google Cloud Run)
- [ ] Team management (multi-user, shared orchestrator)
- [ ] Analytics dashboard (messages/day, response quality)
- [ ] Webhook integrations (Slack notification of high-priority messages)

---

## References

- **Architecture:** [ARCHITECTURE.md](ARCHITECTURE.md)
- **Prior Art:** [PRIOR_ART.md](PRIOR_ART.md)
- **API Reference:** `API.md` — planned, Week 3 Day 3-4 (not yet written)
- **Deployment:** `DEPLOYMENT.md` — planned, Week 3 Day 3-4 (not yet written)

**Ship it.** 🚀
