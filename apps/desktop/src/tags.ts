// Tag name rules shared by the UI and the fixture backend. They mirror
// `clean_name` / `normalize_tag` in vv-persistence (organize.rs).

/** Display form: trimmed, inner whitespace collapsed. */
export function cleanName(s: string): string {
  return s.trim().split(/\s+/u).filter(Boolean).join(" ");
}

/** Uniqueness key: case folded, Turkish İ/I/ı → i, so "İstanbul" = "ISTANBUL". */
export function tagKeyOf(s: string): string {
  return [...cleanName(s)]
    .map((c) => (c === "İ" || c === "I" || c === "ı" ? "i" : c.toLowerCase()))
    .join("")
    .replace(/̇/g, "");
}
