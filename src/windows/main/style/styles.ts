import type { StyleCategory, WritingStyle } from "@/lib/ipc";

export const CATEGORIES: Record<
  StyleCategory,
  { apps: string; label: string; styles: readonly WritingStyle[] }
> = {
  email: {
    apps: "Mail, Outlook, Spark, Superhuman, Gmail and other email apps.",
    label: "Email",
    styles: ["formal", "casual", "excited"],
  },
  other: {
    apps: "Everything else: notes, documents, code editors and browsers.",
    label: "Other",
    styles: ["formal", "casual", "excited"],
  },
  personal: {
    apps: "Messages, WhatsApp, Telegram, Signal, Discord and Messenger.",
    label: "Personal messages",
    styles: ["formal", "casual", "veryCasual"],
  },
  work: {
    apps: "Slack, Microsoft Teams, Mattermost and Google Chat.",
    label: "Work messages",
    styles: ["formal", "casual", "excited"],
  },
};

export const STYLES: Record<
  WritingStyle,
  { description: string; label: string }
> = {
  casual: { description: "Capitals, lighter punctuation.", label: "Casual" },
  excited: {
    description: "More exclamation marks.",
    label: "Excited!",
  },
  formal: { description: "Capitals and full punctuation.", label: "Formal." },
  veryCasual: {
    description: "No capitals, lighter punctuation.",
    label: "very casual",
  },
};

const MESSAGING: readonly StyleCategory[] = ["personal", "work"];

/** "Sounds good, let's meet at noon" as each style writes it in `category`. */
export function styleSample(
  style: WritingStyle,
  category: StyleCategory
): string {
  const end = MESSAGING.includes(category) ? "" : ".";
  switch (style) {
    case "formal":
      return "Sounds good. Let's meet at noon.";
    case "casual":
      return `Sounds good, let's meet at noon${end}`;
    case "veryCasual":
      return `sounds good, let's meet at noon${end}`;
    case "excited":
      return "Sounds good, let's meet at noon!";
    default:
      return style satisfies never;
  }
}
