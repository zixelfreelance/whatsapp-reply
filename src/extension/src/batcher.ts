import type { Capture } from './types';

export interface BatcherOptions {
  maxSize: number;
  /** When set, flush this often even if the batch is short. */
  intervalMs?: number;
  /** How many recent messages to remember for dedup. Defaults to 500. */
  seenLimit?: number;
  onFlush: (batch: Capture[]) => void;
}

/** Stable identity of a message — must match the orchestrator's content_hash inputs. */
function identity(c: Capture): string {
  return [c.platform, c.chatId, c.sender, c.text, c.arrivedAt].join('');
}

export class Batcher {
  private pending: Capture[] = [];
  private seen = new Set<string>();
  private timer?: ReturnType<typeof setInterval>;

  constructor(private readonly opts: BatcherOptions) {
    if (opts.intervalMs !== undefined) {
      this.timer = setInterval(() => this.flush(), opts.intervalMs);
    }
  }

  add(capture: Capture): void {
    const key = identity(capture);
    if (this.seen.has(key)) return;
    this.seen.add(key);

    // Set iterates in insertion order, so the first key is the oldest.
    const limit = this.opts.seenLimit ?? 500;
    while (this.seen.size > limit) {
      const oldest = this.seen.values().next().value;
      if (oldest === undefined) break;
      this.seen.delete(oldest);
    }

    this.pending.push(capture);
    if (this.pending.length >= this.opts.maxSize) {
      this.flush();
    }
  }

  /** No-op when there is nothing pending — an idle tab must not POST empty batches. */
  flush(): void {
    if (this.pending.length === 0) return;
    const batch = this.pending;
    this.pending = [];
    this.opts.onFlush(batch);
  }

  stop(): void {
    if (this.timer !== undefined) clearInterval(this.timer);
  }
}
