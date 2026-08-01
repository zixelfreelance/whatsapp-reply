import { defineConfig } from 'wxt';

// Forked from ../../../zix-freelancer-capture/org/passive-capture/wxt.config.ts.
// Deliberately NOT including @wxt-dev/module-react — this extension has no
// React surface. Approval happens in Telegram; the sidepanel is a status
// viewer only.
export default defineConfig({
  manifest: {
    name: 'whatsapp-reply',
    description:
      'Captures WhatsApp Web messages (read-only) and forwards them to the local orchestrator.',
    permissions: ['storage', 'alarms', 'sidePanel'],
    // Scoped to WhatsApp Web only. passive-capture uses <all_urls>; this
    // sensor must never see another platform — see docs/ESTATE_LESSONS.md §4.
    host_permissions: ['https://web.whatsapp.com/*'],
  },
});
