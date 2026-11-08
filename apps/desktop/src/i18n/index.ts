// Translation keys and locale formatting (SPEC §9.6; STATE.md F2.8; ADR-0018).
//
// Every user-facing string of the new screens is a key in a catalog; the
// English catalog is the only one that ships. A Turkish catalog only needs
// the same keys (`Catalog` makes a missing key a type error). Numbers,
// dates, sizes and plurals go through Intl with the active locale. Titles
// and notes from the library are data and are never translated.

import { en, type Catalog, type MessageKey } from "./en";

export type { MessageKey } from "./en";

const CATALOGS: Record<string, Catalog> = { en };
let locale = "en";

export function setLocale(l: string): void {
  locale = l in CATALOGS ? l : "en";
}

export function getLocale(): string {
  return locale;
}

type Vars = Record<string, string | number>;

function fill(s: string, vars?: Vars): string {
  return vars ? s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(typeof vars[k] === "number" ? formatNumber(vars[k] as number) : vars[k]) : m)) : s;
}

/** The message for `key`, with `{name}` placeholders filled in. */
export function t(key: MessageKey, vars?: Vars): string {
  return fill((CATALOGS[locale] ?? en)[key] ?? en[key], vars);
}

/** Keys that come in `.one` / `.other` pairs. */
export type PluralKey = MessageKey extends infer K ? (K extends `${infer B}.one` ? B : never) : never;

/** `key.one` or `key.other` by the locale's plural rules; `{count}` is filled. */
export function tn(key: PluralKey, count: number, vars?: Vars): string {
  const form = new Intl.PluralRules(locale).select(count) === "one" ? "one" : "other";
  return t(`${key}.${form}` as MessageKey, { count, ...vars });
}

export function formatNumber(n: number): string {
  return new Intl.NumberFormat(locale).format(n);
}

export function formatDateTime(ms: number): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(new Date(ms));
}
