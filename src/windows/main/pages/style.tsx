import type { CleanupLevel, StyleCategory, WritingStyle } from "@/lib/ipc";

import { useCallback, useEffect, useId, useState } from "react";

import { CleanupLevelCards } from "@/components/cleanup-level-cards";
import { Button } from "@/components/ui/button";
import { Radio, RadioGroup } from "@/components/ui/radio-group";
import {
  Select,
  SelectItem,
  SelectPopup,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsList, TabsPanel, TabsTab } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import { listModels, testPolish } from "@/lib/ipc";
import { STYLE_CATEGORIES } from "@/lib/ipc/types";

import { Page, Section } from "../page";
import { hashFor } from "../route";
import { CATEGORIES, STYLES, styleSample } from "../style/styles";
import { useSavedSettings } from "../use-saved-settings";

const SAVED_MS = 2000;
const MODELS_EVENTS = ["models:changed"] as const;

const CATEGORY_ITEMS = STYLE_CATEGORIES.map((value) => ({
  label: CATEGORIES[value].label,
  value,
}));

const isCategory = (value: unknown): value is StyleCategory =>
  STYLE_CATEGORIES.some((c) => c === value);
const isStyle = (value: unknown): value is WritingStyle =>
  typeof value === "string" && value in STYLES;

function StyleCards({
  category,
  value,
  onChange,
}: {
  category: StyleCategory;
  onChange: (category: StyleCategory, style: WritingStyle) => void;
  value: WritingStyle;
}) {
  const change = useCallback(
    (next: unknown) => {
      if (isStyle(next)) {
        onChange(category, next);
      }
    },
    [category, onChange]
  );
  return (
    <RadioGroup
      aria-label={`Style for ${CATEGORIES[category].label.toLowerCase()}`}
      className="grid grid-cols-3 gap-2"
      onValueChange={change}
      value={value}
    >
      {CATEGORIES[category].styles.map((style) => (
        // biome-ignore lint/a11y/noLabelWithoutControl: the Radio inside is the control
        <label
          className="flex cursor-pointer flex-col gap-1 rounded-xl border bg-background px-3 py-2.5 transition-colors hover:bg-accent has-data-checked:border-foreground/40 has-data-checked:bg-background-muted motion-reduce:transition-none"
          key={style}
        >
          <span className="flex items-center gap-2 font-medium text-foreground text-sm">
            <Radio value={style} />
            {STYLES[style].label}
          </span>
          <span className="text-foreground-muted text-xs">
            {STYLES[style].description}
          </span>
          <span className="pt-1 font-redaction text-base text-foreground">
            {styleSample(style, category)}
          </span>
        </label>
      ))}
    </RadioGroup>
  );
}

function TryIt({ initialCategory }: { initialCategory: StyleCategory }) {
  const inputId = useId();
  const [text, setText] = useState("");
  const [category, setCategory] = useState<StyleCategory>(initialCategory);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  const changeText = useCallback(
    (event: React.ChangeEvent<HTMLTextAreaElement>) =>
      setText(event.target.value),
    []
  );
  const changeCategory = useCallback((next: unknown) => {
    if (isCategory(next)) {
      setCategory(next);
    }
  }, []);
  const run = useCallback(
    async (event: React.FormEvent) => {
      event.preventDefault();
      if (!text.trim()) {
        return;
      }
      setPending(true);
      setError(null);
      try {
        setResult(await testPolish(text, category));
      } catch (caught) {
        setResult(null);
        setError(
          caught instanceof Error ? caught.message : "Couldn't try that."
        );
      } finally {
        setPending(false);
      }
    },
    [category, text]
  );

  return (
    <Section
      description="Type something the way you'd say it and see what Tiny Wispr would write."
      title="Try it"
    >
      <form className="flex flex-col gap-3" onSubmit={run}>
        <label className="sr-only" htmlFor={inputId}>
          What you'd say
        </label>
        <Textarea
          id={inputId}
          onChange={changeText}
          placeholder="um so can we move the call to friday i think that works better"
          rows={3}
          value={text}
        />
        <div className="flex items-center justify-between gap-3">
          <Select
            items={CATEGORY_ITEMS}
            onValueChange={changeCategory}
            value={category}
          >
            <SelectTrigger aria-label="Write it as" className="w-56" size="sm">
              <SelectValue />
            </SelectTrigger>
            <SelectPopup>
              {CATEGORY_ITEMS.map((item) => (
                <SelectItem key={item.value} value={item.value}>
                  {item.label}
                </SelectItem>
              ))}
            </SelectPopup>
          </Select>
          <Button
            disabled={!text.trim()}
            loading={pending}
            size="sm"
            type="submit"
          >
            Try it
          </Button>
        </div>
      </form>
      <div aria-live="polite">
        {result === null ? null : (
          <p className="whitespace-pre-wrap rounded-xl border bg-background-muted px-4 py-3 text-foreground text-sm">
            {result}
          </p>
        )}
        {error ? (
          <p className="text-destructive-foreground text-sm">{error}</p>
        ) : null}
      </div>
    </Section>
  );
}

export function StylePage() {
  const { settings, save } = useSavedSettings();
  const models = useIpcQuery(listModels, MODELS_EVENTS);
  const [tab, setTab] = useState<StyleCategory>("personal");
  const [saved, setSaved] = useState(0);

  useEffect(() => {
    if (saved === 0) {
      return;
    }
    const timer = setTimeout(() => setSaved(0), SAVED_MS);
    return () => clearTimeout(timer);
  }, [saved]);

  const changeLevel = useCallback(
    (cleanupLevel: CleanupLevel) => save({ cleanupLevel }),
    [save]
  );
  const changeStyle = useCallback(
    (category: StyleCategory, style: WritingStyle) => {
      if (!settings) {
        return;
      }
      save({ styles: { ...settings.styles, [category]: style } });
      setSaved(Date.now());
    },
    [save, settings]
  );
  const changeTab = useCallback((next: unknown) => {
    if (isCategory(next)) {
      setTab(next);
    }
  }, []);

  if (!settings) {
    return <Page title="Style" />;
  }

  const languageModel = models.data?.find(
    (m) =>
      m.kind === "language" &&
      m.id === settings.llmModel &&
      m.status.state === "downloaded"
  );
  const hasLanguageModel = models.data?.some(
    (m) => m.kind === "language" && m.status.state === "downloaded"
  );

  return (
    <Page
      description="How Tiny Wispr tidies up what you say before it pastes it."
      title="Style"
    >
      <Section
        description="How much Tiny Wispr changes your words."
        title="Auto cleanup"
      >
        <CleanupLevelCards
          onValueChange={changeLevel}
          value={settings.cleanupLevel}
        />
        {models.data && !languageModel ? (
          <p className="text-pretty text-foreground-muted text-sm">
            Light uses built-in rules. Medium needs a language model.{" "}
            <a
              className="text-foreground underline underline-offset-2"
              href={hashFor("models")}
            >
              {hasLanguageModel ? "Choose one in Models" : "Get one in Models"}
            </a>
          </p>
        ) : null}
      </Section>

      <Section
        action={
          <span aria-live="polite" className="text-foreground-muted text-xs">
            {saved ? "Saved" : ""}
          </span>
        }
        description="Tiny Wispr matches the app you're dictating into."
        title="Writing style"
      >
        <Tabs onValueChange={changeTab} value={tab}>
          <TabsList className="self-start">
            {STYLE_CATEGORIES.map((category) => (
              <TabsTab key={category} value={category}>
                {CATEGORIES[category].label}
              </TabsTab>
            ))}
          </TabsList>
          {STYLE_CATEGORIES.map((category) => (
            <TabsPanel
              className="flex flex-col gap-3 pt-3"
              key={category}
              value={category}
            >
              <StyleCards
                category={category}
                onChange={changeStyle}
                value={settings.styles[category]}
              />
              <p className="text-pretty text-foreground-muted text-xs">
                {CATEGORIES[category].apps}
              </p>
            </TabsPanel>
          ))}
        </Tabs>
      </Section>

      <TryIt initialCategory={tab} />
    </Page>
  );
}
