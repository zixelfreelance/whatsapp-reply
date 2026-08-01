/** An inbound message exactly as the extension observed it. */
export interface Capture {
  /**
   * Always `"whatsapp"`. Sent explicitly and asserted by the orchestrator —
   * a LinkedIn sensor once wrote private DMs into a sibling's lake because
   * nothing checked. See docs/ESTATE_LESSONS.md §4.
   */
  platform: 'whatsapp';
  chatId: string;
  sender: string;
  text: string;
  /**
   * The message's real arrival time, ISO-8601. Threaded end to end and never
   * re-stamped downstream — the rate governor's min-age check depends on it.
   */
  arrivedAt: string;
}
