const CONTROLS = "button, a[href], input, [tabindex]:not([tabindex='-1'])";
/** A failed or undone removal leaves the row in place; stop watching then. */
const GIVE_UP_MS = 10_000;

/**
 * Keeps keyboard focus in the list when `row` (an `<li>`) is about to be
 * removed: once it is gone, focuses the same control in the next row (or
 * the previous one), or the list's heading when the list is now empty.
 * Without this, focus falls back to the top of the page.
 *
 * `from` is the control that started the removal; it defaults to the
 * focused element and decides which control of the neighbour gets focus.
 */
export function focusAfterRemoval(
  row: Element | null | undefined,
  from: Element | null = document.activeElement
) {
  if (!(row instanceof HTMLElement && row.parentElement)) {
    return;
  }
  const controls = [...row.querySelectorAll(CONTROLS)];
  const index = Math.max(0, from ? controls.indexOf(from) : 0);
  const neighbour = row.nextElementSibling ?? row.previousElementSibling;
  const section = row.closest("section");
  const headings = [
    section?.querySelector<HTMLElement>("h2"),
    document.querySelector<HTMLElement>("h1"),
  ];

  const observer = new MutationObserver(() => {
    if (row.isConnected) {
      return;
    }
    stop();
    if (neighbour?.isConnected) {
      const target =
        neighbour.querySelectorAll<HTMLElement>(CONTROLS)[index] ??
        neighbour.querySelector<HTMLElement>(CONTROLS);
      if (target) {
        target.focus();
        return;
      }
    }
    // The section goes away with its last row on some pages.
    const heading = headings.find((h) => h?.isConnected);
    if (heading) {
      heading.tabIndex = -1;
      heading.focus();
    }
  });
  const timer = setTimeout(() => stop(), GIVE_UP_MS);
  function stop() {
    observer.disconnect();
    clearTimeout(timer);
  }
  observer.observe(document.body, { childList: true, subtree: true });
}
