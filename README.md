# whatsapp-reply

**Claude-powered WhatsApp auto-reply system** — capture messages, analyze with Claude, send replies via browser automation.

## What It Does

```
WhatsApp Message → Extension captures → jcode analyzes
                                           ↓
                                      Claude API
                                           ↓
                                    agent-browser executes
                                           ↓
                                    Reply sent ✅
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

# Install agent-browser
npm install -g agent-browser
agent-browser install

# Build extension
cd src/extension
npm install
npm run build
# Load dist/ in chrome://extensions

# Start orchestrator
cd ../..
cargo run --release
```

### Run

1. Open Chrome → `chrome://extensions` → Load unpacked `src/extension/dist/`
2. Start backend: `cargo run --release`
3. Open WhatsApp Web → send a test message
4. Extension captures it → Claude replies automatically

## Architecture

**Three layers:**

1. **Capture** (Extension) — Chrome MV3 extension, detects WhatsApp messages, batches, sends to backend
2. **Orchestrator** (Rust) — HTTP API, manages action queue, spawns Claude + agent-browser loops
3. **Execution** (agent-browser) — Takes screenshots, gets semantic refs, executes clicks/type

**Stack:**
- Rust: Tokio async runtime, Axum HTTP server, SQLite persistence
- TypeScript: React sidepanel UI, Chrome MV3 extension
- Claude API: Vision + reasoning
- agent-browser: Browser automation CLI

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
│   ├── ARCHITECTURE.md        # System design
│   ├── PRIOR_ART.md           # Analysis of passive-capture, zix-agent, jcode
│   ├── ROADMAP.md             # 3-week implementation plan
│   ├── API.md                 # Orchestrator endpoints
│   └── DEPLOYMENT.md          # Production checklist
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
port = 9999
db = "data/whatsapp-reply.db"
log_level = "info"

[claude]
api_key = "${CLAUDE_API_KEY}"
model = "claude-opus-4-1"

[agent_browser]
timeout_ms = 10000
max_retries = 3
```

Or set `CLAUDE_API_KEY` env var:

```bash
export CLAUDE_API_KEY=sk-ant-...
cargo run --release
```

## API

### POST /action/execute

Send an action to execute (capture a message, generate reply, send).

```bash
curl -X POST http://127.0.0.1:9999/action/execute \
  -H "Content-Type: application/json" \
  -d '{
    "action": "send_whatsapp",
    "to": "+1234567890",
    "message": "Hi there!"
  }'
```

Response:

```json
{
  "status": "pending",
  "id": "uuid-...",
  "created_at": "2026-08-01T03:00:00Z"
}
```

### GET /audit?date=2026-08-01

Query audit trail (messages, actions, results).

```bash
curl http://127.0.0.1:9999/audit?date=2026-08-01
```

## Development

### Build Extension

```bash
cd src/extension
npm install
npm run build      # TypeScript → dist/
npm run watch      # Auto-rebuild on changes
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
3. Open WhatsApp Web
4. Send test message to yourself
5. Check sidepanel → message captured
6. Wait 30 seconds → Claude reply sent
7. Verify in SQLite: `sqlite3 data/whatsapp-reply.db "SELECT * FROM actions LIMIT 5;"`

### 24-Hour Stability Test

See [ROADMAP.md](docs/ROADMAP.md#phase-5-validation--hardening-week-4-5).

## Troubleshooting

**Extension not capturing?**
- Check `chrome://extensions` → extension enabled
- Check console for errors (F12)
- Verify orchestrator running: `curl http://127.0.0.1:9999/health`

**Claude not replying?**
- Check `CLAUDE_API_KEY` set: `echo $CLAUDE_API_KEY`
- Check logs: `tail -f logs/orchestrator-*.log | grep claude`
- Check rate limit: Claude API quota exhausted

**agent-browser failing?**
- Check installed: `which agent-browser`
- Check Chrome: `agent-browser open --url https://web.whatsapp.com`
- Check timeout: increase `timeout_ms` in config

## Prior Art

This project is built on:

- **passive-capture** — Capture pipeline, extension architecture, SQLite schema
- **zix-agent** — Server scaffolding (Axum, Tokio, logging)
- **jcode** — AI agent orchestration

See [PRIOR_ART.md](docs/PRIOR_ART.md) for detailed analysis.

## Roadmap

**3-week sprint to MVP:**

- **Week 1:** Orchestrator HTTP API + extension message capture
- **Week 2:** End-to-end loop (message → Claude → reply sent)
- **Week 3:** 24-hour stability test + multi-platform support

See [ROADMAP.md](docs/ROADMAP.md) for detailed phases.

## Cost

- **Claude API:** ~$3/month (1000 messages, $0.003 per action)
- **Infrastructure:** Zero (runs locally)
- **Time:** 3 weeks (you build it)

## License

MIT

## Contact

GitHub: [@zixelfreelance](https://github.com/zixelfreelance)

---

**Ship it.** 🚀
