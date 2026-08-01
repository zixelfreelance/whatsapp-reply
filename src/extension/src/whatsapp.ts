import type { Capture } from './types';

/**
 * The raw fields a DOM extraction must produce before it can become a Capture.
 * Deliberately all-strings: this is the boundary between "we read some DOM"
 * and "we have a message."
 */
export interface RawExtraction {
  chatId: string;
  sender: string;
  text: string;
  arrivedAt: string;
}

/**
 * Validate a raw extraction and stamp the platform.
 *
 * Throws rather than emitting a partial capture. An endpoint being captured is
 * not the same as the data being present (FPIOS lesson #4) — a half-extracted
 * message that slips through becomes a draft replying to nothing.
 */
export function toCapture(raw: RawExtraction): Capture {
  for (const field of ['chatId', 'sender', 'text', 'arrivedAt'] as const) {
    if (!raw[field]?.trim()) {
      throw new Error(`whatsapp extraction is missing ${field}`);
    }
  }

  if (Number.isNaN(Date.parse(raw.arrivedAt))) {
    throw new Error(`whatsapp extraction has an unparseable timestamp: ${raw.arrivedAt}`);
  }

  return {
    platform: 'whatsapp',
    chatId: raw.chatId,
    sender: raw.sender,
    text: raw.text,
    arrivedAt: raw.arrivedAt,
  };
}

/**
 * Read one rendered message element into a RawExtraction.
 *
 * ⚠️ NOT IMPLEMENTED — blocked on a real DOM fixture.
 *
 * Writing selectors against invented markup is precisely how `capture-reddit`
 * lost an entire adapter design: its adapters targeted a `.json` contract that
 * never appears during passive browsing, and the interceptor would have
 * captured zero content. See docs/ESTATE_LESSONS.md §2.
 *
 * To implement: open WhatsApp Web, capture real message elements into
 * `spike/fixtures/*.html`, then drive this function from those fixtures the
 * way `capture-reddit/probe/fixtures/` drives its adapter tests.
 *
 * The Day 0 spike must also answer whether an *unopened* chat's message text
 * exists in the DOM at all — if it doesn't, this function is the wrong shape
 * entirely and the agent has to open chats to read them.
 */
export function extractFromElement(_el: Element): RawExtraction {
  throw new Error(
    'extractFromElement is not implemented: needs real WhatsApp DOM fixtures from the Day 0 spike',
  );
}
