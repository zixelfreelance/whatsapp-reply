# Cross-Pollination: zix-agent ↔ passive-capture ↔ whatsapp-reply

Three repos chasing the same shape — capture something, reason about it, act. This is what
each has that the others should take, in both directions.

Surveyed 2026-08-01: `~/zix-agent`, `~/zix-freelancer-capture/org/passive-capture`, plus
`capture-reddit` where it settles a question the other two can't.

Companion to [ESTATE_LESSONS.md](ESTATE_LESSONS.md), which covers *lessons*. This one is
about **code and seams** — things to copy, not just learn from.

---

## Part 1 — What whatsapp-reply should take from zix-agent

zix-agent is the most architecturally mature of the three. Three of its seams map onto
things this repo has not yet built, which means they can be copied rather than invented.

### 1.1 The `AgentBackend` trait + a self-identifying echo stub ⭐ take this first

`crates/zix-jcode-bridge/src/agent_loop.rs`

```rust
#[async_trait]
pub trait AgentBackend: Send + Sync {
    async fn ask(&self, session_id: &str, message: &str) -> Result<AgentReply>;
}
```

Two implementors: `EchoBackend` (the **default**) and `JcodeBackend` (the real one, behind
`ZIX_AGENT_BACKEND=jcode`). Its own doc comment states why:

> The echo stub remains the **default** so the transport, the Lake and the command bus can be
> exercised end to end on a machine with no `jcode` install and no provider login — CI included.

**Why this matters here more than there.** whatsapp-reply still has to build the Telegram bot
and the executor. Both need *a draft* to move through the pipeline, and neither needs that
draft to be any good. An echo backend means the whole loop — capture → draft → Telegram card →
approve → send — can be exercised without spending a single API call, and without a key in CI.

And this detail is worth stealing verbatim:

> It is intentionally honest: the reply says it is an echo, so a stub answer can never be
> mistaken for a real one in a screenshot or a test fixture.

In a system whose output is **sent to a real person**, a stub draft that looked real and got
approved would be mortifying. `[echo:model] …` in the text makes that impossible.

**Action:** `claude.rs` becomes a trait with `EchoDrafter` (default) and `ClaudeDrafter`
(real, requires `CLAUDE_API_KEY`). Build Telegram and the executor against the echo.

### 1.2 The subprocess-driving pattern for `agent-browser`

`crates/zix-jcode-bridge/src/jcode.rs` shells out to a CLI, parses `--json` output, and maps
session ids. That is *structurally identical* to what the executor must do with
`agent-browser`. Three details worth copying:

- **`pub const DEFAULT_BIN: &str = "jcode"`, overridable** — "so tests can point at a stub."
  This is how the executor gets tested without a real browser. Without it, executor tests
  need Chrome.
- **Documented verification date and real output.** The module header pastes the actual JSON
  jcode emitted, with "Verified against jcode 2026-08-01." When the CLI changes, you know what
  you'd verified against. `agent-browser` shipped 117 versions in 7 months — this discipline
  is not optional for us.
- **Honest scope limits.** "The map is in memory, so continuity is per-process — a restart
  starts a new jcode session. Persisting it belongs in the Lake, once there is a reason to."

### 1.3 The command bus + behaviour ledger ⭐ this is the approval seam

`crates/zix-server/src/bus.rs`

> The command bus — the only path by which state changes. Both transports (HTTP, Tauri IPC)
> deserialise a `Command` and hand it here. Every command is written to the behaviour ledger
> **before** its outcome is returned, so the audit trail cannot drift from what actually ran.

This is precisely the seam recommended during this repo's design session: *build the approval
**protocol**, treat surfaces as thin clients over it.* whatsapp-reply has Telegram now and
sidepanel/Tauri later; without a bus those become three code paths that drift.

The ordering comment is a lesson in itself:

> Ask first: a failed turn should not leave a user message stranded in history with no reply.

Our equivalent: **record the approval before attempting the send**, so a crash mid-send leaves
an auditable "approved, outcome unknown" rather than a silent gap.

### 1.4 `ConnectorRegistry` — and the warning attached to it

`crates/zix-server/src/connectors/mod.rs` carries a scar:

> This is the "register in `server.rs`, done" half of the hexagonal claim — without it, an
> adapter can exist and still be unreachable, which is exactly how `PassiveCaptureConnector`
> shipped as **dead code**.

An abstraction without a registration point produces code that compiles, tests, and never
runs. Worth remembering when the Telegram and agent-browser adapters land here. (Their
unknown-connector test literally uses `"telegram"` as its example.)

---

## Part 2 — What whatsapp-reply can give back

This repo is younger, but three things here don't exist in either sibling.

### 2.1 The rate governor ⭐ give this back

`src/orchestrator/src/governor.rs` — 10 tests, no WhatsApp-specific knowledge in it.

zix-agent has `Command::ConnectorAction` → `connector.act()` with **no pacing whatsoever**.
That is fine while connectors only read. The moment one posts, messages, or bids on the
user's behalf, it needs exactly this: minimum age, minimum gap, a rolling-hour ceiling, and
jitter — with the latest release time winning so two constraints can't cancel out.

The governor is generic over what it paces. Lifting it into `zix-core` would serve every
connector in the estate.

### 2.2 The `INSERT OR IGNORE` trap

Found while building the queue here, and it is a **silent** one:

> `INSERT OR IGNORE` swallows a `CHECK` violation as a no-op and returns 0 rows affected. Code
> that reads "0 rows ⇒ duplicate" therefore reports a *constraint violation* as a *duplicate*.

Any sibling that combines `OR IGNORE` dedup with a `CHECK` constraint has this bug latent.
Given the estate's history — a LinkedIn sensor writing DMs into an Upwork lake — a foreign
capture being quietly filed as "duplicate" instead of erroring is the exact failure mode
nobody wants twice. The fix is an explicit guard before the insert, keeping the `CHECK` as
defence in depth.

### 2.3 Threaded time, and never stamping mid-pipeline

`capture-reddit` root-caused this; this repo encoded it as an invariant and a test. Worth
auditing the siblings for `Utc::now()` / `Date.now()` called during projection rather than
threaded from the source event — it breaks replay determinism and, wherever pacing exists,
quietly defeats it.

---

## Part 3 — The finding that unblocks this repo's extension

**`passive-capture`'s extension does not build**, and never has:

```json
"build": "echo 'no frontend bundle step yet — cockpit build wires this up' && exit 0"
```

It has a `wxt.config.ts`, but its extension is still a hand-written `manifest.json` pointing at
raw paths (`main/relay.ts`, `sw/ingest.ts`). It is **mid-migration** from manual MV3 to WXT,
stopped partway.

This explains a real defect in this repo: "fork `passive-capture`'s Bun+WXT setup" produced a
config with no `entrypoints/`, so `wxt build` fails with *no entrypoints found*. There was
nothing to copy, because the source has never built.

**`capture-reddit` is the sibling to copy instead.** It has a genuine WXT build:

| | passive-capture | capture-reddit |
|---|---|---|
| `entrypoints/` | ✗ | ✓ `background.ts`, `capture.content.ts`, `capture-main.content.ts`, `offscreen/`, `sidepanel/` |
| `build` script | `echo … && exit 0` | `wxt build` |
| Built output | — | `.output/chrome-mv3` |
| Host permissions | `<all_urls>` | `*://*.reddit.com/*` (scoped) |

Its structure also answers a question this repo will hit: `capture.content.ts` (ISOLATED) plus
`capture-main.content.ts` (MAIN world) is the standard split, and MAIN-world injection is the
documented fallback if the ISOLATED world can't reach what it needs — the same fallback
`capture-fiver` pre-declared for its React fill.

**Action:** model `src/extension/entrypoints/` on `capture-reddit`, not `passive-capture`. Fix
the source repo's build too, or mark it clearly as non-building so the next person doesn't
fork it expecting one.

---

## Priority

| # | Action | Direction | Why now |
|---|---|---|---|
| 1 | `AgentBackend` trait + honest echo stub | ← zix-agent | Unblocks Telegram + executor with zero API spend |
| 2 | Copy `capture-reddit`'s entrypoint layout | ← capture-reddit | This repo's extension currently cannot build |
| 3 | Subprocess pattern w/ overridable binary | ← zix-agent | Executor testable without Chrome |
| 4 | Command bus before the second approval surface | ← zix-agent | Cheap now, a refactor later |
| 5 | Lift the governor into shared code | → zix-agent | Needed the moment any connector acts outward |
| 6 | Warn siblings about the `OR IGNORE` trap | → both | Silent, and in the estate's known failure class |
| 7 | Flag `passive-capture`'s non-building extension | → passive-capture | It will be forked again otherwise |
