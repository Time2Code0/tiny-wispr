/** Mirrors the limits the backend enforces, so mistakes show while typing. */
export const MAX_PHRASE_CHARS = 60;
export const MAX_EXPANSION_CHARS = 4000;
/** The expansion counter appears from here on. */
export const EXPANSION_COUNTER_FROM = 3600;

const same = (a: string, b: string) =>
  a.trim().toLocaleLowerCase() === b.trim().toLocaleLowerCase();

/** A word, misspelling or trigger: not empty, at most 60 characters. */
export function phraseIssue(label: string, value: string): string | null {
  if (!value.trim()) {
    return `${label} can't be empty`;
  }
  if (value.trim().length > MAX_PHRASE_CHARS) {
    return `${label} can be up to ${MAX_PHRASE_CHARS} characters`;
  }
  return null;
}

/** Whether another item (not `selfId`) already uses `value`, ignoring case. */
export function isDuplicate<T extends { id: string }>(
  items: readonly T[],
  pick: (item: T) => string,
  value: string,
  selfId: string | null
): boolean {
  return items.some((item) => item.id !== selfId && same(pick(item), value));
}

export function expansionIssue(value: string): string | null {
  if (!value.trim()) {
    return "Expansion can't be empty";
  }
  if (value.length > MAX_EXPANSION_CHARS) {
    return `Expansion can be up to ${MAX_EXPANSION_CHARS.toLocaleString("en")} characters`;
  }
  return null;
}

/** Case-insensitive match against any of the given fields. */
export function matchesSearch(
  query: string,
  ...fields: (string | null)[]
): boolean {
  const needle = query.trim().toLocaleLowerCase();
  return (
    !needle ||
    fields.some((field) => field?.toLocaleLowerCase().includes(needle))
  );
}
