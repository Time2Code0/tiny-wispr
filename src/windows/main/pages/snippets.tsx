import type { Snippet } from "@/lib/ipc";

import { useCallback, useMemo, useRef, useState } from "react";

import {
  Delete02Icon,
  PencilEdit02Icon,
  TextIcon,
} from "@hugeicons/core-free-icons";
import { cn } from "cn";

import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Field, FieldError, FieldLabel } from "@/components/ui/field";
import { Icon } from "@/components/ui/icon";
import { Input } from "@/components/ui/input";
import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { Textarea } from "@/components/ui/textarea";
import { toastManager } from "@/components/ui/toast";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import { detectPlatform } from "@/lib/hotkey";
import { deleteSnippet, listSnippets, upsertSnippet } from "@/lib/ipc";

import { EntryList } from "../entry-list";
import { focusAfterRemoval } from "../focus-after-removal";
import { IconButton } from "../icon-button";
import { Page, Section } from "../page";
import {
  EXPANSION_COUNTER_FROM,
  expansionIssue,
  isDuplicate,
  MAX_EXPANSION_CHARS,
  matchesSearch,
  phraseIssue,
} from "../validation";

interface Draft {
  expansion: string;
  id: string | null;
  trigger: string;
}

const EMPTY: Draft = { expansion: "", id: null, trigger: "" };

const EXAMPLES: Draft[] = [
  { expansion: "name@example.com", id: null, trigger: "my email" },
  {
    expansion: "Thanks for reaching out. I'll get back to you by tomorrow.",
    id: null,
    trigger: "quick reply",
  },
];

const pickTrigger = (snippet: Snippet) => snippet.trigger;
const SAVE_KEY = detectPlatform() === "macos" ? "⌘" : "Ctrl";

function triggerIssue(draft: Draft, snippets: readonly Snippet[]) {
  const issue = phraseIssue("Trigger", draft.trigger);
  if (issue) {
    return issue;
  }
  return isDuplicate(snippets, pickTrigger, draft.trigger, draft.id)
    ? "A snippet with this trigger already exists"
    : null;
}

function SnippetRow({
  snippet,
  onEdit,
  onDelete,
}: {
  onDelete: (snippet: Snippet) => void;
  onEdit: (snippet: Snippet) => void;
  snippet: Snippet;
}) {
  const edit = useCallback(() => onEdit(snippet), [snippet, onEdit]);
  const remove = useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      focusAfterRemoval(event.currentTarget.closest("li"), event.currentTarget);
      onDelete(snippet);
    },
    [snippet, onDelete]
  );
  return (
    <li className="group flex items-center gap-3 px-4 py-2.5 transition-colors focus-within:bg-background-muted hover:bg-background-muted motion-reduce:transition-none">
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-foreground text-sm">
          {snippet.trigger}
        </span>
        <span className="truncate text-foreground-muted text-xs">
          {snippet.expansion.replaceAll("\n", " ")}
        </span>
      </div>
      <div className="flex shrink-0 gap-0.5 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100 motion-reduce:transition-none">
        <IconButton
          icon={PencilEdit02Icon}
          label={`Edit ${snippet.trigger}`}
          onClick={edit}
        />
        <IconButton
          icon={Delete02Icon}
          label={`Delete ${snippet.trigger}`}
          onClick={remove}
        />
      </div>
    </li>
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
      {example.trigger}
    </Button>
  );
}

export function SnippetsPage() {
  const snippets = useIpcQuery(listSnippets);
  const list = snippets.data ?? [];
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [submitted, setSubmitted] = useState(false);
  const [serverError, setServerError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [search, setSearch] = useState("");
  const triggerInput = useRef<HTMLInputElement>(null);
  const expansionInput = useRef<HTMLTextAreaElement>(null);

  const triggerProblem = triggerIssue(draft, list);
  const expansionProblem = expansionIssue(draft.expansion);
  const showTriggerProblem =
    triggerProblem && (submitted || draft.trigger.trim() !== "")
      ? triggerProblem
      : null;
  const showExpansionProblem =
    expansionProblem &&
    (submitted || draft.expansion.length > MAX_EXPANSION_CHARS)
      ? expansionProblem
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
    triggerInput.current?.focus();
  }, []);

  const edit = useCallback(
    (snippet: Snippet) =>
      load({
        expansion: snippet.expansion,
        id: snippet.id,
        trigger: snippet.trigger,
      }),
    [load]
  );

  const remove = useCallback(
    async (snippet: Snippet) => {
      try {
        await deleteSnippet(snippet.id);
        await snippets.refetch();
        if (draft.id === snippet.id) {
          reset();
        }
        toastManager.add({
          actionProps: {
            children: "Undo",
            onClick: () => {
              upsertSnippet({
                expansion: snippet.expansion,
                id: null,
                trigger: snippet.trigger,
              })
                .then(() => snippets.refetch())
                .catch(() =>
                  toastManager.add({
                    title: `Couldn't bring back ${snippet.trigger}`,
                    type: "error",
                  })
                );
            },
          },
          title: `${snippet.trigger} deleted`,
        });
      } catch {
        toastManager.add({
          title: `Couldn't delete ${snippet.trigger}`,
          type: "error",
        });
      }
    },
    [draft.id, reset, snippets]
  );

  const save = useCallback(async () => {
    setSubmitted(true);
    if (triggerProblem) {
      triggerInput.current?.focus();
      return;
    }
    if (expansionProblem) {
      expansionInput.current?.focus();
      return;
    }
    setSaving(true);
    setServerError(null);
    try {
      await upsertSnippet({
        expansion: draft.expansion,
        id: draft.id,
        trigger: draft.trigger.trim(),
      });
      await snippets.refetch();
      toastManager.add({
        title: draft.id ? "Snippet updated" : "Snippet added",
        type: "success",
      });
      reset();
    } catch (caught) {
      setServerError(
        caught instanceof Error ? caught.message : "Couldn't save the snippet"
      );
    } finally {
      setSaving(false);
    }
  }, [draft, expansionProblem, reset, snippets, triggerProblem]);

  const submit = useCallback(
    (event: React.FormEvent) => {
      event.preventDefault();
      save();
    },
    [save]
  );
  const onExpansionKey = useCallback(
    (event: React.KeyboardEvent) => {
      if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        save();
      }
    },
    [save]
  );

  const changeTrigger = useCallback(
    (event: React.ChangeEvent<HTMLInputElement>) => {
      const trigger = event.target.value;
      setDraft((d) => ({ ...d, trigger }));
      setServerError(null);
    },
    []
  );
  const changeExpansion = useCallback(
    (event: React.ChangeEvent<HTMLTextAreaElement>) => {
      const expansion = event.target.value;
      setDraft((d) => ({ ...d, expansion }));
      setServerError(null);
    },
    []
  );

  const shown = useMemo(
    () =>
      (snippets.data ?? []).filter((s) =>
        matchesSearch(search, s.trigger, s.expansion)
      ),
    [snippets.data, search]
  );
  const count = list.length;
  const { length } = draft.expansion;

  return (
    <Page
      description="Say a short phrase and Tiny Wispr types something longer. Say “my email” and it writes your address."
      title="Snippets"
    >
      <Section title={draft.id ? "Edit snippet" : "Add a snippet"}>
        <form
          className="flex flex-col gap-4 rounded-xl border bg-background p-4"
          noValidate
          onSubmit={submit}
        >
          <Field invalid={Boolean(showTriggerProblem)}>
            <FieldLabel>When you say</FieldLabel>
            <Input
              onChange={changeTrigger}
              placeholder="my email"
              ref={triggerInput}
              type="text"
              value={draft.trigger}
            />
            <FieldError match>{showTriggerProblem}</FieldError>
          </Field>
          <Field invalid={Boolean(showExpansionProblem ?? serverError)}>
            <div className="flex items-baseline justify-between gap-2">
              <FieldLabel>Type this</FieldLabel>
              {length >= EXPANSION_COUNTER_FROM ? (
                <span
                  aria-live="polite"
                  className={cn(
                    "text-xs tabular-nums",
                    length > MAX_EXPANSION_CHARS
                      ? "text-destructive-foreground"
                      : "text-foreground-muted"
                  )}
                >
                  {length.toLocaleString()} /{" "}
                  {MAX_EXPANSION_CHARS.toLocaleString()}
                </span>
              ) : null}
            </div>
            <Textarea
              onChange={changeExpansion}
              onKeyDown={onExpansionKey}
              placeholder="name@example.com"
              ref={expansionInput}
              rows={4}
              value={draft.expansion}
            />
            <FieldError match>{showExpansionProblem ?? serverError}</FieldError>
          </Field>
          <div className="flex items-center justify-end gap-2">
            <span className="me-auto flex items-center gap-1.5 text-foreground-muted text-xs">
              <KbdGroup>
                <Kbd>{SAVE_KEY}</Kbd>
                <Kbd>Enter</Kbd>
              </KbdGroup>
              saves
            </span>
            {draft.id ? (
              <Button onClick={reset} type="button" variant="ghost">
                Cancel
              </Button>
            ) : null}
            <Button loading={saving} type="submit">
              {draft.id ? "Save" : "Add snippet"}
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
                <Icon aria-hidden="true" icon={TextIcon} />
              </EmptyMedia>
              <EmptyTitle>No snippets yet</EmptyTitle>
              <EmptyDescription>
                Good for addresses, links and replies you send often. Try one of
                these:
              </EmptyDescription>
            </EmptyHeader>
            <EmptyContent className="flex-row justify-center gap-2">
              {EXAMPLES.map((example) => (
                <ExampleChip
                  example={example}
                  key={example.trigger}
                  onPick={load}
                />
              ))}
            </EmptyContent>
          </Empty>
        }
        error={Boolean(snippets.error)}
        loaded={snippets.data !== undefined}
        noun="snippets"
        onSearch={setSearch}
        search={search}
        shown={shown.length}
        title="Your snippets"
      >
        {shown.map((snippet) => (
          <SnippetRow
            key={snippet.id}
            onDelete={remove}
            onEdit={edit}
            snippet={snippet}
          />
        ))}
      </EntryList>
    </Page>
  );
}
