import { version } from "../../../../package.json";
import { Section } from "../page";

const CREDITS = [
  {
    name: "whisper.cpp",
    what: "Speech recognition, by Georgi Gerganov and contributors. MIT licence.",
  },
  {
    name: "llama.cpp",
    what: "Runs the language models, by Georgi Gerganov and contributors. MIT licence.",
  },
  {
    name: "Whisper",
    what: "The speech models, by OpenAI. MIT licence.",
  },
  {
    name: "Tauri",
    what: "The app framework. MIT or Apache 2.0 licence.",
  },
];

export function AboutSettings() {
  return (
    <>
      <div className="-mt-2 flex flex-col gap-1">
        <p className="font-redaction text-2xl text-foreground">Tiny Wispr</p>
        <p className="text-foreground-muted text-sm">
          Version <span className="font-mono">{version}</span>
        </p>
        <p className="text-pretty text-foreground-muted text-sm">
          Local, private dictation. No account, no cloud, no tracking.
        </p>
      </div>
      <Section title="Built on">
        <ul className="flex flex-col divide-y rounded-xl border">
          {CREDITS.map((credit) => (
            <li className="flex flex-col gap-0.5 px-4 py-3" key={credit.name}>
              <span className="text-foreground text-sm">{credit.name}</span>
              <span className="text-pretty text-foreground-muted text-xs">
                {credit.what}
              </span>
            </li>
          ))}
        </ul>
      </Section>
    </>
  );
}
