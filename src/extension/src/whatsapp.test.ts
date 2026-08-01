import { describe, expect, test } from 'vitest';
import { toCapture } from './whatsapp';

const valid = {
  chatId: '98912xxxxxxx@c.us',
  sender: '+98912xxxxxxx',
  text: 'can you do Friday?',
  arrivedAt: '2026-08-01T03:00:00.000Z',
};

describe('toCapture', () => {
  test('accepts a fully-populated extraction and stamps the platform', () => {
    const capture = toCapture(valid);
    expect(capture).toEqual({ platform: 'whatsapp', ...valid });
  });

  test.each(['chatId', 'sender', 'text', 'arrivedAt'] as const)(
    'rejects an extraction missing %s rather than passing an empty field downstream',
    (field) => {
      // FPIOS lesson: an endpoint being captured is not the data being
      // present. A half-extracted message must fail loudly here, not become
      // a draft replying to nothing.
      expect(() => toCapture({ ...valid, [field]: '' })).toThrow(/missing/i);
    },
  );

  test('rejects a timestamp that is not a real instant', () => {
    expect(() => toCapture({ ...valid, arrivedAt: 'yesterday-ish' })).toThrow(
      /timestamp/i,
    );
  });
});
