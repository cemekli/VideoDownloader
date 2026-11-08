// Link detection in the UI (STATE.md F1.8). Only decides whether to offer
// Add URL; the backend validates and normalizes (`check_url`, `save_url`).

/** Text that is almost certainly a link (offer Add URL instead of searching for it). */
export function looksLikeUrl(text: string): boolean {
  return /^(https?:\/\/|www\.)\S+$/i.test(text.trim());
}

/** `www.example.com/x` → `https://www.example.com/x`; anything else unchanged. */
export function withScheme(text: string): string {
  const t = text.trim();
  return /^www\./i.test(t) ? `https://${t}` : t;
}
