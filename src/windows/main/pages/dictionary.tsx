import type { DictionaryEntry } from "@/lib/ipc";

import { useCallback, useId, useMemo, useRef, useState } from "react";

import {
  ArrowRight01Icon,
  Book02Icon,
  Delete02Icon,
  PencilEdit02Icon,
} from "@hugeicons/core-free-icons";

import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
} from "@/components/ui/field";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { toastManager } from "@/components/ui/toast";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import {
  deleteDictionaryEntry,
  listDictionary,
  upsertDictionaryEntry,
} from "@/lib/ipc";

import { EntryList } from "../entry-list";
import { focusAfterRemoval } from "../focus-after-removal";
import { IconButton } from "../icon-button";
import { Page, Section } from "../page";
import { isDuplicate, matchesSearch, phraseIssue } from "../validation";

interface Draft {
  correcting: boolean;
  id: string | null;
  misspelling: string;
  word: string;
}

const EMPTY: Draft = { correcting: false, id: null, misspelling: "", word: "" };

const EXAMPLES: Draft[] = [
  { correcting: false, id: null, misspelling: "", word: "Kubernetes" },
  { correcting: true, id: null, misspelling: "post grass", word: "Postgres" },
];

const pickWord = (entry: DictionaryEntry) => entry.word;

function wordIssue(
  draft: Draft,
  entries: readonly DictionaryEntry[]
): string | null {
  const issue = phraseIssue("Word", draft.word);
  if (issue) {
    return issue;
  }
  return isDuplicate(entries, pickWord, draft.word, draft.id)
    ? "This word is already in your dictionary"
    : null;
}

function misspellingIssue(draft: Draft): string | null {
  if (!draft.correcting) {
    return null;
  }
  const issue = phraseIssue("Wrong spelling", draft.misspelling);
  if (issue) {
    return issue;
  }
  return draft.misspelling.trim().toLocaleLowerCase() ===
    draft.word.trim().toLocaleLowerCase()
    ? "The wrong spelling has to differ from the word"
    : null;
}

function EntryRow({
  entry,
  onEdit,
  onDelete,
}: {
  entry: DictionaryEntry;
  onDelete: (entry: DictionaryEntry) => void;
  onEdit: (entry: DictionaryEntry) => void;
}) {
  const edit = useCallback(() => onEdit(entry), [entry, onEdit]);
  const remove = useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      focusAfterRemoval(event.currentTarget.closest("li"), event.currentTarget);
      onDelete(entry);
    },
    [entry, onDelete]
  );
  return (
    <li className="group flex items-center gap-3 px-4 py-2 transition-colors focus-within:bg-background-muted hover:bg-background-muted motion-reduce:transition-none">
      <p className="flex min-w-0 flex-1 items-center gap-2 text-sm">
        {entry.misspelling ? (
          <>
            <span className="truncate text-foreground-muted line-through decoration-foreground-muted/40">
              {entry.misspelling}
            </span>
            <Icon
              aria-hidden="true"
              className="size-3.5 shrink-0 text-foreground-muted"
              icon={ArrowRight01Icon}
            />
            <span className="sr-only">becomes</span>
          </>
        ) : null}
        <span className="truncate text-foreground">{entry.word}</span>
      </p>
      <div className="flex shrink-0 gap-0.5 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100 motion-reduce:transition-none">
        <IconButton
          icon={PencilEdit02Icon}
          label={`Edit ${entry.word}`}
          onClick={edit}
        />
        <IconButton
          icon={Delete02Icon}
          label={`Delete ${entry.word}`}
          onClick={remove}
        />
      </div>
    </li>
  );
}

export function DictionaryPage() {
  const entries = useIpcQuery(listDictionary);
  const list = entries.data ?? [];
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [submitted, setSubmitted] = useState(false);
  const [serverError, setServerError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [search, setSearch] = useState("");
  const wordInput = useRef<HTMLInputElement>(null);
  const misspellingInput = useRef<HTMLInputElement>(null);
  const switchId = useId();

  const wordProblem = wordIssue(draft, list);
  const misspellingProblem = misspellingIssue(draft);
  const showWordProblem =
    wordProblem && (submitted || draft.word.trim() !== "") ? wordProblem : null;
  const showMisspellingProblem =
    misspellingProblem && (submitted || draft.misspelling.trim() !== "")
      ? misspellingProblem
      : null;

  const reset = useCallback(() => {
    setDraft(EMPTY);
    setSubmitted(false);
    setServerError(null);
  }, []);

  const load = useCallback((next: Draft) => {
    setDraft(next);
    setSubmitted(false);
    setServerError(null);
    wordInput.current?.focus();
  }, []);

  const edit = useCallback(
    (entry: DictionaryEntry) =>
      load({
        correcting: entry.misspelling !== null,
        id: entry.id,
        misspelling: entry.misspelling ?? "",
        word: entry.word,
      }),
    [load]
  );

  const remove = useCallback(
    async (entry: DictionaryEntry) => {
      try {
        await deleteDictionaryEntry(entry.id);
        await entries.refetch();
        if (draft.id === entry.id) {
          reset();
        }
        toastManager.add({
          actionProps: {
            children: "Undo",
            onClick: () => {
              upsertDictionaryEntry({
                id: null,
                misspelling: entry.misspelling,
                word: entry.word,
              })
                .then(() => entries.refetch())
                .catch(() =>
                  toastManager.add({
                    title: `Couldn't bring back ${entry.word}`,
                    type: "error",
                  })
                );
            },
          },
          title: `${entry.word} deleted`,
        });
      } catch {
        toastManager.add({
          title: `Couldn't delete ${entry.word}`,
          type: "error",
        });
      }
    },
    [draft.id, entries, reset]
  );

  const submit = useCallback(
    async (event: React.FormEvent) => {
      event.preventDefault();
      setSubmitted(true);
      if (wordProblem) {
        wordInput.current?.focus();
        return;
      }
      if (misspellingProblem) {
        misspellingInput.current?.focus();
        return;
      }
      setSaving(true);
      setServerError(null);
      try {
        await upsertDictionaryEntry({
          id: draft.id,
          misspelling: draft.correcting ? draft.misspelling.trim() : null,
          word: draft.word.trim(),
        });
        await entries.refetch();
        toastManager.add({
          title: draft.id ? "Word updated" : `${draft.word.trim()} added`,
          type: "success",
        });
        reset();
      } catch (caught) {
        setServerError(
          caught instanceof Error ? caught.message : "Couldn't save the word"
        );
      } finally {
        setSaving(false);
      }
    },
    [draft, entries, misspellingProblem, reset, wordProblem]
  );

  const changeWord = useCallback(
    (event: React.ChangeEvent<HTMLInputElement>) => {
      const word = event.target.value;
      setDraft((d) => ({ ...d, word }));
      setServerError(null);
    },
    []
  );
  const changeMisspelling = useCallback(
    (event: React.ChangeEvent<HTMLInputElement>) => {
      const misspelling = event.target.value;
      setDraft((d) => ({ ...d, misspelling }));
      setServerError(null);
    },
    []
  );
  const changeCorrecting = useCallback(
    (correcting: boolean) => setDraft((d) => ({ ...d, correcting })),
    []
  );

  const shown = useMemo(
    () =>
      (entries.data ?? []).filter((e) =>
        matchesSearch(search, e.word, e.misspelling)
      ),
    [entries.data, search]
  );

  const count = list.length;

  return (
    <Page
      description="Names, jargon and other words Tiny Wispr should know. Add a misspelling to fix a word it keeps getting wrong."
      title="Dictionary"
    >
      <Section title={draft.id ? "Edit word" : "Add a word"}>
        <form
          className="flex flex-col gap-4 rounded-xl border bg-background p-4"
          noValidate
          onSubmit={submit}
        >
          <Field invalid={Boolean(showWordProblem ?? serverError)}>
            <FieldLabel>Word or phrase</FieldLabel>
            <Input
              onChange={changeWord}
              placeholder="Kubernetes"
              ref={wordInput}
              type="text"
              value={draft.word}
            />
            <FieldError match>{showWordProblem ?? serverError}</FieldError>
          </Field>
          <div className="flex items-center gap-3">
            <Switch
              checked={draft.correcting}
              id={switchId}
              onCheckedChange={changeCorrecting}
            />
            <label className="text-foreground text-sm" htmlFor={switchId}>
              Correct a misspelling
            </label>
          </div>
          {draft.correcting ? (
            <Field invalid={Boolean(showMisspellingProblem)}>
              <FieldLabel>Wrong spelling</FieldLabel>
              <Input
                onChange={changeMisspelling}
                placeholder="post grass"
                ref={misspellingInput}
                type="text"
                value={draft.misspelling}
              />
              <FieldDescription>
                What Tiny Wispr writes now. It becomes the word above.
              </FieldDescription>
              <FieldError match>{showMisspellingProblem}</FieldError>
            </Field>
          ) : null}
          <div className="flex justify-end gap-2">
            {draft.id ? (
              <Button onClick={reset} type="button" variant="ghost">
                Cancel
              </Button>
            ) : null}
            <Button loading={saving} type="submit">
              {draft.id ? "Save" : "Add word"}
            </Button>
          </div>
        </form>
      </Section>

      <EntryList
        count={count}
        empty={
          <Empty className="rounded-xl border md:py-12">
            <EmptyHeader className="gap-2">
              <EmptyMedia variant="icon">
                <Icon aria-hidden="true" icon={Book02Icon} />
              </EmptyMedia>
              <EmptyTitle>No words yet</EmptyTitle>
              <EmptyDescription>
                Add the names and terms you use most. Try one of these:
              </EmptyDescription>
            </EmptyHeader>
            <EmptyContent className="flex-row justify-center gap-2">
              {EXAMPLES.map((example) => (
                <ExampleChip
                  example={example}
                  key={example.word}
                  onPick={load}
                />
              ))}
            </EmptyContent>
          </Empty>
        }
        error={Boolean(entries.error)}
        loaded={entries.data !== undefined}
        noun="words"
        onSearch={setSearch}
        search={search}
        shown={shown.length}
        title="Your words"
      >
        {shown.map((entry) => (
          <EntryRow
            entry={entry}
            key={entry.id}
            onDelete={remove}
            onEdit={edit}
          />
        ))}
      </EntryList>
    </Page>
  );
}

function ExampleChip({
  example,
  onPick,
}: {
  example: Draft;
  onPick: (draft: Draft) => void;
}) {
  const pick = useCallback(() => onPick(example), [example, onPick]);
  return (
    <Button
      onClick={pick}
      shape="full"
      size="sm"
      type="button"
      variant="outline"
    >
      {example.correcting
        ? `${example.misspelling} → ${example.word}`
        : example.word}
    </Button>
  );
}
