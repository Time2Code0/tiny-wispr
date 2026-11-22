import type { CleanupLevel, StyleCategory, WritingStyle } from "./types";

const WHITESPACE = /\s+/g;
const TERMINAL = /[.!?…]$/;
const FINAL_PERIOD = /\.$/;

/**
 * A rough stand-in for the rule-based pipeline behind `test_polish`: it
 * capitalises and sets the final punctuation by style, so the Style page's
 * example changes in the browser. The app does much more.
 */
export function approximatePolish(
  text: string,
  level: CleanupLevel,
  style: WritingStyle,
  category: StyleCategory
): string {
  const body = text.replace(WHITESPACE, " ").trim();
  if (level === "none" || body === "") {
    return text;
  }
  const capitalised = body.charAt(0).toUpperCase() + body.slice(1);
  const terminated = TERMINAL.test(capitalised)
    ? capitalised
    : `${capitalised}.`;
  switch (style) {
    case "casual":
      return category === "personal" || category === "work"
        ? terminated.replace(FINAL_PERIOD, "")
        : terminated;
    case "veryCasual":
      return (terminated.charAt(0).toLowerCase() + terminated.slice(1)).replace(
        FINAL_PERIOD,
        ""
      );
    case "excited":
      return terminated.replace(FINAL_PERIOD, "!");
    default:
      return terminated;
  }
}
