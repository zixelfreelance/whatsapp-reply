import { describe, expect, test } from 'vitest';
import { Batcher } from './batcher';
import type { Capture } from './types';

function capture(text: string, at = '2026-08-01T03:00:00.000Z'): Capture {
  return {
    platform: 'whatsapp',
    chatId: '98912xxxxxxx@c.us',
    sender: '+98912xxxxxxx',
    text,
    arrivedAt: at,
  };
}

describe('Batcher', () => {
  test('flushes once five captures have accumulated', () => {
    const flushed: Capture[][] = [];
    const b = new Batcher({ maxSize: 5, onFlush: (batch) => flushed.push(batch) });

    for (let i = 0; i < 4; i++) b.add(capture(`m${i}`));
    expect(flushed).toHaveLength(0);

    b.add(capture('m4'));

    expect(flushed).toHaveLength(1);
    expect(flushed[0]).toHaveLength(5);
  });

  test('flushes on the interval even when the batch is short', () => {
    vi.useFakeTimers();
    const flushed: Capture[][] = [];
    const b = new Batcher({
      maxSize: 5,
      intervalMs: 2000,
      onFlush: (batch) => flushed.push(batch),
    });

    b.add(capture('only one'));
    expect(flushed).toHaveLength(0);

    vi.advanceTimersByTime(2000);

    expect(flushed).toHaveLength(1);
    expect(flushed[0]).toHaveLength(1);
    vi.useRealTimers();
  });

  test('never emits an empty batch', () => {
    // An idle tab would otherwise POST an empty array every interval.
    vi.useFakeTimers();
    const flushed: Capture[][] = [];
    const b = new Batcher({
      maxSize: 5,
      intervalMs: 2000,
      onFlush: (batch) => flushed.push(batch),
    });

    vi.advanceTimersByTime(10_000);

    expect(flushed).toHaveLength(0);
    vi.useRealTimers();
  });

  test('drops a message it has already seen', () => {
    // WhatsApp Web re-renders constantly (receipts, presence, reactions,
    // virtualised scroll), so the observer re-emits the same message. The
    // orchestrator dedupes too, but suppressing it here saves the round trip.
    const flushed: Capture[][] = [];
    const b = new Batcher({ maxSize: 2, onFlush: (batch) => flushed.push(batch) });

    b.add(capture('can you do Friday?'));
    b.add(capture('can you do Friday?'));
    expect(flushed).toHaveLength(0);

    b.add(capture('different message'));

    expect(flushed).toHaveLength(1);
    expect(flushed[0]!.map((c) => c.text)).toEqual([
      'can you do Friday?',
      'different message',
    ]);
  });

  test('forgets the oldest messages so memory cannot grow without bound', () => {
    // This runs in an MV3 service worker for days. An unbounded seen-set is a
    // slow leak. Eviction can let a very old message back through, which is
    // fine — the orchestrator's content_hash UNIQUE is the authority.
    const flushed: Capture[][] = [];
    const b = new Batcher({
      maxSize: 100,
      seenLimit: 3,
      onFlush: (batch) => flushed.push(batch),
    });

    b.add(capture('first'));
    b.add(capture('second'));
    b.add(capture('third'));
    b.add(capture('fourth')); // evicts 'first'

    b.add(capture('first')); // no longer remembered, so accepted again
    b.flush();

    expect(flushed[0]!.map((c) => c.text)).toEqual([
      'first',
      'second',
      'third',
      'fourth',
      'first',
    ]);
  });
});
