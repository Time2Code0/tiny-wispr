export type NoteBlock =
  | { kind: "heading"; text: string }
  | { items: string[]; kind: "list" }
  | { kind: "paragraph"; text: string };

const HEADING = /^#{1,6}\s+/;
const BULLET = /^[-*]\s+/;
/** `**bold**`, `` `code` `` and `[text](url)` become their text. */
const INLINE = /\*\*(.+?)\*\*|`([^`]+)`|\[([^\]]+)\]\([^)]*\)/g;

function plain(text: string): string {
  return text.replace(INLINE, (_, bold, code, link) => bold ?? code ?? link);
}

/**
 * Release notes are Markdown written by `git-cliff`: headings, bullet lists
 * and short paragraphs. This keeps that structure and drops the rest, so no
 * HTML from the update server is ever rendered.
 */
export function noteBlocks(notes: string): NoteBlock[] {
  const blocks: NoteBlock[] = [];
  for (const raw of notes.split("\n")) {
    const line = raw.trim();
    if (!line) {
      continue;
    }
    if (HEADING.test(line)) {
      blocks.push({ kind: "heading", text: plain(line.replace(HEADING, "")) });
      continue;
    }
    if (BULLET.test(line)) {
      const item = plain(line.replace(BULLET, ""));
      const last = blocks.at(-1);
      if (last?.kind === "list") {
        last.items.push(item);
      } else {
        blocks.push({ items: [item], kind: "list" });
      }
      continue;
    }
    blocks.push({ kind: "paragraph", text: plain(line) });
  }
  return blocks;
}
