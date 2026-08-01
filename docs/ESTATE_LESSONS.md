# Estate Lessons

What seven sibling repos in `~/zix-freelancer-capture/org/` already learned, and what it
means for `whatsapp-reply`. Read before Week 1.

Sources surveyed 2026-08-01: `gmail`, `pastely-app`,
`freelancer-platform-intelligence-os` (FPIOS), `capture-reddit`, `capture-producthunt`,
`capture-fiver`, `captur-chromestore`.

This project is **sensor #12** in an estate that already holds ~17k captures. Almost every
mistake available here has been made next door.

---

## 1. The one that should change the plan: passive capture only sees what you browse

`captur-chromestore` names it directly:

> Passive capture and full coverage are in **direct tension** — passive only ever sees what
> you browse; a human can't browse 200k+ entities.

For a marketplace crawler that's a coverage problem. **For a 24/7 auto-reply system it may
be a correctness problem.** WhatsApp Web renders the *open* conversation. If a message
arrives in a chat you don't have open, does its text exist in the DOM at all — or only a
sidebar preview with an unread badge?

Nothing in `ARCHITECTURE.md` addresses this, and the whole product assumes the agent notices
messages while you're away. Three possible realities:

| Reality | Consequence |
|---|---|
| Full text is in the DOM for all chats | No problem. Capture as designed. |
| Only a truncated sidebar preview | Draft quality collapses — Claude replies to half a sentence |
| Nothing until the chat is opened | **The agent must open chats to read them** — which marks them read, changes your visible state, and turns "passive capture" into an active behaviour with its own detection profile |

**Action: this belongs in the Day 0 spike, not Week 1.** It's cheaper to answer than the
`agent-browser` questions and it invalidates more if it goes badly.

---

## 2. Probe before you plan — `capture-reddit` lost a whole design to this

`capture-reddit` wrote adapters against Reddit's legacy `.json` Listing contract. Then
someone actually probed the live site (`probe/findings.md`):

> **Live Reddit (shreddit) delivers post & comment content as HTML, not JSON.** The legacy
> `.json` Listing contract the plan's adapters were written against **never appears during
> passive browsing.**

Five plan assumptions invalidated at once — the interceptor filtered for JSON and would have
captured **zero** content. The probe cost a day; the plan it corrected was worth a week.

Two practices worth copying verbatim:

- **Save fixtures as evidence.** `probe/fixtures/*.sample.html`, plus the full 400 KB raw
  response. Later, the integration tests run against those real fixtures rather than
  hand-written mocks.
- **End the probe with an explicit recommendation and a named gate** — "this is the decision
  for Lead," Option A vs Option B, with a suggested call.

Our Day 0 spike should produce the same artefacts: `spike/findings.md`, saved DOM fixtures,
and a yes/no per question.

**Our version of the same trap, already caught:** `PRIOR_ART.md` claimed `passive-capture`
had a reusable WhatsApp detector. It's two lines of hostname matching. Same failure mode,
caught by inspection rather than a probe — which is luck, not process.

---

## 3. `capture-fiver` already built the exact smoke test we need

`capture-fiver/docs/live-smoke-react-fill.md` — "T1.8, the linchpin gate" — answers *can a
programmatic value make a React form register it?* Its shape is directly reusable:

```js
function nativeSetter(el){
  const proto = el instanceof HTMLTextAreaElement
    ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  return Object.getOwnPropertyDescriptor(proto, 'value').set;
}
function setReactInputValue(el, value){
  nativeSetter(el).call(el, value);
  el.dispatchEvent(new Event('input',  { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
  el.dispatchEvent(new Event('blur',   { bubbles: true }));
}
```

**Why their verdict test is clever, and worth stealing wholesale:**

> A React-controlled input resets its `.value` back to component state on the next render
> *unless* an `input`/`change` event was processed as a genuine change. So if the test text
> is still there 400ms later, React accepted it.

Wait-then-check-it-stuck is a real signal. "No exception thrown" is not.

They also pre-declared the fallback: if it fails, **inject from MAIN world and bridge to the
ISOLATED relay via `window.postMessage`**. Decide the fallback before you need it.

### ⚠️ But it does not transfer unmodified

WhatsApp's composer is a **`contenteditable` div** (Lexical/Draft-family), not an `<input>`
or `<textarea>`. `HTMLInputElement.prototype.value`'s setter **does not exist on it**. The
Fiverr snippet will fail on WhatsApp for the wrong reason and tell you nothing.

The contenteditable equivalents to test, in order:

1. `document.execCommand('insertText', false, text)` on the focused composer — legacy, but
   still the most widely-working path for editor frameworks
2. A synthetic `beforeinput` / `input` pair with `inputType: 'insertText'`
3. A synthetic `ClipboardEvent('paste')` with the text in its `DataTransfer`

Keep their **structure** — find the target, snapshot before-state, act, wait 400ms, assert it
stuck, print a loud PASS/FAIL — and swap the mechanism. That's the Day 0 script.

---

## 4. The cross-contamination incident — read this one twice

From `pastely-app/CLAUDE.md` (ADR 0009):

> The estate holds **11 capture sensors and ~17k captures**, every one a fork of the same
> brain — which is how a **LinkedIn sensor wrote private DMs into this repo's Upwork-only
> lake** (2026-07-12).

A sensor forked from a shared spine wrote **private messages into the wrong repo's
database**. That already happened, in this estate, to these people.

`whatsapp-reply` is sensor #12, forking the same lineage, and it handles the **most sensitive
data of any of them** — private personal conversations, not public job posts.

Non-negotiables that follow:

- **Its own database file, its own port, its own lake.** Never share, never "unify later."
  `PRIOR_ART.md`'s open question about sharing a schema with `passive-capture` is answered:
  **no.**
- **Assert the platform on ingest.** A capture without `platform == "whatsapp"` is rejected,
  not coerced. Cheap, and it's precisely the check whose absence caused the incident.
- **The estate-wide cure is `fl-core` / `painpointly` — "one spine, N adapters."** We are not
  adopting it (it's mid-build), so we get isolation the blunt way instead.

---

## 5. Convergent evidence for decisions we already made

Two `pastely-app` "spine invariants, never break" independently match this session's calls:

| Their invariant | Our decision |
|---|---|
| *"Collector captures; the backend decides"* — the extension only yields raw captures; all projection/scoring/intelligence lives behind the local lake | Extension is **read-only**; drafting, governor, and approval live in the orchestrator |
| *"Raw is sacred"* — a re-capture is a no-op; a schema change is a re-projection, never a re-fetch | Keep the inbound message verbatim; drafts are derived and re-derivable |

Reaching the same place from different reasoning is the good kind of confirmation.

---

## 6. Dedup on content hash — or the `MutationObserver` will drown us

FPIOS lesson #5:

> `raw_captures.sha256` is UNIQUE over the payload. Re-visiting a page you've already
> captured produces an **identical payload → no new row**.

`passive-capture` does SHA-256 content hashing for the same reason. **This is not optional
for us** — WhatsApp Web re-renders its message list constantly (read receipts, presence,
reactions, virtualised scrolling). A naive `MutationObserver` will re-emit the same message
many times.

Without dedup, in our design, a duplicate capture becomes a **duplicate draft**, which
becomes a **duplicate Telegram card**, which — if approved twice — becomes the **same reply
sent twice**. That's both an annoyance and a bot-shaped signal.

`content_hash UNIQUE` on `(chat_id, sender, text, timestamp)` belongs in the schema from day
one, not as a Week 2 fix.

---

## 7. Determinism: don't stamp time at projection

`capture-reddit/docs/VERIFICATION.md` documents a subtle bug and its root-cause fix:

> The adapters previously did `const now = Date.now()` at projection time […] Re-projecting
> the same raw row later therefore produced different timestamps and a different view_event
> id (so replay INSERTed a duplicate instead of replacing — a latent bug). Fixed by threading
> the envelope's real capture time.

Note they fixed the cause rather than mocking the clock in tests — and only then could they
claim replay-determinism honestly.

Ours: thread the message's real arrival time from capture through drafting into the record.
Never `Date.now()` mid-pipeline. `min_age_seconds` in the rate governor depends on knowing
when the message *actually arrived*, so this is load-bearing for account safety, not just
tidiness.

---

## 8. Verification: split automated from manual, and say why

`capture-reddit/docs/VERIFICATION.md` is the best testing doc in the estate. It states
plainly what cannot be tested headlessly:

> Chrome extensions require a loaded unpacked build, a live offscreen document, and live
> `chrome.*` APIs that don't exist under Node/Vitest.

…then gives **exact, runnable manual steps** for those criteria — including killing the
service worker mid-browse and confirming the capture count keeps climbing across its death.

Their automated tests use the **real** SQLite engine, the **real** schema, and a **real**
captured fixture — nothing mocked at the SQL layer, so a missing FTS5 build would fail
loudly rather than silently pass.

Our `ROADMAP.md` testing section should adopt the same split. Our manual-only list:
content-script capture, MV3 service-worker death, the `agent-browser` send, and Telegram
round-trip.

---

## 9. Captured ≠ usable

FPIOS lesson #4:

> `/api/projects/0.1/bids` was hit 11× but came back **empty every time** […] **an endpoint
> being captured ≠ the data being present.**

Our equivalent: a `MutationObserver` firing is not the same as having `chat_id`, sender, and
full text. The Day 0 spike must assert on **extracted field values**, not on "the observer
fired."

`gmail`'s probe is the positive version of this — it decoded one real thread row field by
field and produced a table of exactly what a capture yields. That's the standard.

---

## 10. Port and repo conventions the estate already follows

- **Ports are allocated, not chosen:** captur `8799`/`8787`, linkedin `8788`, insta `8789`,
  fiver `8790`, FPIOS `8810`. FPIOS's README says the number is *deliberate* so siblings
  coexist on one host. **Our `9999` was picked arbitrarily** — move into the `88xx`/`881x`
  band and record it.
- **Every mature sibling has a repo-level `CLAUDE.md`** with Mission / Status / Locked
  decisions / Guardrails, explicitly written to make the repo self-orienting *without*
  org-level access. **We don't have one.** After this session's decisions, we have the
  material for it.
- **`STATUS.md` as a cold-start page** (`capture-fiver`) — what we started, the path taken,
  what exists now, the open question, and the exact commands to resume.
- **Local-only git until the stream un-parks** (`capture-producthunt`) — creating a public
  remote is treated as an outward action, deliberately deferred. We pushed to a **public**
  repo on day one; worth a conscious second look given the subject matter.

---

## 11. The uncomfortable one: `pastely-app` chose *against* Rust

Our Week-1 recommendation was "keep Rust, because the prior art is Rust and copies cleanly."
The estate's most commercially advanced product ran a spike and went the other way:

> **The brain is NOT ported to Rust** (ratified 2026-07-09, spike + ADR 0001/0006/0008).
> shahab-app's Rust `core/` and `fl-core` are **sidelined**.

Their shape: a **thin Rust Tauri v2 shell** (window, keychain, licence, sidecar lifecycle)
around a **TypeScript brain compiled to a single Bun binary**, talking over loopback with a
per-install token.

FPIOS is the counterexample — Rust + axum + SQLite, and it works. So the estate has run both
experiments and neither is obviously wrong.

**What actually changes for us:** the argument for Rust here was *cheap copy-adapt from
`passive-capture`*. Section 2 of this document weakened that considerably — the WhatsApp
extractor is greenfield, and `trace.rs` plus queue concepts is a smaller prize than it looked.
If the Day 0 spike is written in TypeScript anyway (it must be — it runs in a browser
console), a Bun brain keeps the whole project in one language.

**Not re-deciding it here.** Flagging that the justification is thinner than when it was made,
and that the estate has direct experience on both sides. Revisit once Day 0 lands.

---

## 12. Guardrails worth importing wholesale

From `capture-producthunt/CLAUDE.md`:

> - The money is **Upwork / Pastely**. This is a *parallel* foundation experiment — it parks
>   the moment it competes with the Pastely build.
> - Don't build a data graveyard: a foundation with no daily-used lens is failure, not
>   progress.

Two questions this project has never been asked:

1. **Where does `whatsapp-reply` sit against Pastely?** The estate runs an explicit WIP=1
   discipline, and `capture-producthunt` is parked behind a validated sale. Nobody has said
   whether this one is parked, parallel, or ahead.
2. **What is our "daily-used lens"?** Ours is concrete and easy to check: *do you approve
   drafts every day?* If the Telegram cards go unanswered for a week, the project has failed
   regardless of how well the pipeline works. That's a better success metric than the
   latency and success-rate targets currently in `ARCHITECTURE.md`.

---

## Action list

| # | Action | Where |
|---|---|---|
| 1 | Add "is unopened-chat text in the DOM?" to the spike — **highest-invalidation question** | `ROADMAP.md` Day 0 |
| 2 | Rewrite the fill smoke test for `contenteditable`, keeping fiver's stuck-value verdict | `spike/` |
| 3 | Save fixtures + `spike/findings.md` with an explicit gate recommendation | `spike/` |
| 4 | `content_hash UNIQUE` in the schema | `ARCHITECTURE.md` |
| 5 | Thread capture time end-to-end; never `Date.now()` mid-pipeline | `ARCHITECTURE.md` |
| 6 | Own DB, own port (move off 9999), assert `platform == "whatsapp"` on ingest | config + `queue.rs` |
| 7 | Answer `PRIOR_ART.md`'s shared-schema question: **no** | `PRIOR_ART.md` |
| 8 | Split verification into automated vs manual, with runnable manual steps | `ROADMAP.md` |
| 9 | Write a repo-level `CLAUDE.md` | root |
| 10 | Add the daily-use success metric | `ARCHITECTURE.md` |
| 11 | Decide where this sits against Pastely (WIP=1) | `CLAUDE.md` |
| 12 | Revisit Rust-vs-Bun after Day 0 | — |
