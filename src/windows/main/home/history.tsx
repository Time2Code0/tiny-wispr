import type { HistoryEntry, HistoryRetention, Hotkey } from "@/lib/ipc";

import { useCallback, useEffect, useMemo, useState } from "react";

import {
  ArrowTurnBackwardIcon,
  ArrowTurnForwardIcon,
  Copy01Icon,
  Delete02Icon,
  Mic01Icon,
  Search01Icon,
  Tick02Icon,
} from "@hugeicons/core-free-icons";
import { cn } from "cn";

import { ShortcutKeys } from "@/components/shortcut-keys";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Icon } from "@/components/ui/icon";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group";
import { Skeleton } from "@/components/ui/skeleton";
import { toastManager } from "@/components/ui/toast";
import { useIpcEvent } from "@/hooks/use-ipc-event";
import { deleteHistoryEntry } from "@/lib/ipc";

import { focusAfterRemoval } from "../focus-after-removal";
import { IconButton } from "../icon-button";
import { hashFor } from "../route";
import { groupByDay, wasEdited } from "./format";
import { useDebounced, useHistory } from "./use-history";

const SEARCH_DEBOUNCE_MS = 200;
const COPIED_MS = 1500;
/** Longer than this and the row starts clamped, with "Show more". */
const CLAMP_CHARS = 220;

const timeOf = (ms: number) =>
  new Date(ms).toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });

function HistoryRow({
  entry,
  onDelete,
  onAnnounce,
}: {
  entry: HistoryEntry;
  onAnnounce: (text: string) => void;
  onDelete: (entry: HistoryEntry) => void;
}) {
  const edited = wasEdited(entry);
  const [showRaw, setShowRaw] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const text = showRaw ? entry.rawText : entry.finalText;
  const long = text.length > CLAMP_CHARS;

  useEffect(() => {
    if (!copied) {
      return;
    }
    const timer = setTimeout(() => setCopied(false), COPIED_MS);
    return () => clearTimeout(timer);
  }, [copied]);

  const copy = useCallback(() => {
    navigator.clipboard
      .writeText(text)
      .then(() => {
        setCopied(true);
        onAnnounce("Copied");
      })
      .catch(() => {
        toastManager.add({ title: "Couldn't copy the text", type: "error" });
      });
  }, [onAnnounce, text]);

  const toggleRaw = useCallback(() => {
    setShowRaw((raw) => !raw);
    onAnnounce(showRaw ? "Showing the edited text" : "Showing what you said");
  }, [onAnnounce, showRaw]);

  const remove = useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      focusAfterRemoval(event.currentTarget.closest("li"), event.currentTarget);
      onDelete(entry);
    },
    [entry, onDelete]
  );
  const toggleExpanded = useCallback(() => setExpanded((open) => !open), []);

  return (
    <li className="group flex gap-4 rounded-lg px-3 py-3 transition-colors focus-within:bg-background-muted hover:bg-background-muted motion-reduce:transition-none">
      <time
        className="w-16 shrink-0 pt-px text-foreground-muted text-xs tabular-nums"
        dateTime={new Date(entry.createdAt).toISOString()}
      >
        {timeOf(entry.createdAt)}
      </time>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <p
          className={cn(
            "whitespace-pre-wrap text-pretty text-foreground text-sm",
            long && !expanded && "line-clamp-3"
          )}
        >
          {text}
        </p>
        <p className="flex flex-wrap items-center gap-x-2 text-foreground-muted text-xs">
          {entry.app ? <span>{entry.app.name}</span> : null}
          {edited ? <span>{showRaw ? "What you said" : "Edited"}</span> : null}
          {long ? (
            <button
              aria-expanded={expanded}
              className="rounded-sm text-foreground underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-foreground"
              onClick={toggleExpanded}
              type="button"
            >
              {expanded ? "Show less" : "Show more"}
            </button>
          ) : null}
        </p>
      </div>
      <div className="-my-1 flex shrink-0 items-start gap-0.5 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100 motion-reduce:transition-none">
        <IconButton
          icon={copied ? Tick02Icon : Copy01Icon}
          label={copied ? "Copied" : "Copy"}
          onClick={copy}
        />
        {edited ? (
          <IconButton
            icon={showRaw ? ArrowTurnForwardIcon : ArrowTurnBackwardIcon}
            label={showRaw ? "Redo AI edit" : "Undo AI edit"}
            onClick={toggleRaw}
          />
        ) : null}
        <IconButton icon={Delete02Icon} label="Delete" onClick={remove} />
      </div>
    </li>
  );
}

function HistorySkeleton() {
  return (
    <div aria-hidden="true" className="flex flex-col gap-3">
      <Skeleton className="h-4 w-20" />
      {[0, 1, 2].map((row) => (
        <div className="flex gap-4 px-3 py-3" key={row}>
          <Skeleton className="h-3 w-16" />
          <div className="flex flex-1 flex-col gap-2">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-3 w-24" />
          </div>
        </div>
      ))}
    </div>
  );
}

function HistoryOff() {
  return (
    <Empty className="rounded-xl border md:py-12">
      <EmptyHeader className="gap-2">
        <EmptyTitle>History is off</EmptyTitle>
        <EmptyDescription>
          Dictations aren't kept on this computer.{" "}
          <a href={hashFor("settings", "privacy")}>Change this in Privacy</a>
        </EmptyDescription>
      </EmptyHeader>
    </Empty>
  );
}

function TeachEmpty({ pushToTalk }: { pushToTalk: Hotkey | undefined }) {
  return (
    <Empty className="rounded-xl border md:py-12">
      <EmptyHeader className="gap-2">
        <EmptyMedia variant="icon">
          <Icon aria-hidden="true" icon={Mic01Icon} />
        </EmptyMedia>
        <EmptyTitle>Your dictations will appear here</EmptyTitle>
        <EmptyDescription className="flex flex-wrap items-center justify-center gap-1.5">
          Hold
          {pushToTalk ? <ShortcutKeys hotkey={pushToTalk} /> : null}
          in any app and speak. Release to paste.
        </EmptyDescription>
      </EmptyHeader>
    </Empty>
  );
}

/** History, newest first, grouped by day, with search and load more. */
export function HistorySection({
  retention,
  pushToTalk,
}: {
  pushToTalk: Hotkey | undefined;
  retention: HistoryRetention | undefined;
}) {
  const [search, setSearch] = useState("");
  const query = useDebounced(search, SEARCH_DEBOUNCE_MS);
  const [version, setVersion] = useState(0);
  useIpcEvent("history:changed", () => setVersion((v) => v + 1));
  const history = useHistory(query, version);
  const [pending, setPending] = useState<ReadonlySet<string>>(new Set());
  const [announcement, setAnnouncement] = useState("");
  const changeSearch = useCallback(
    (event: React.ChangeEvent<HTMLInputElement>) =>
      setSearch(event.target.value),
    []
  );

  const unhide = useCallback((id: string) => {
    setPending((previous) => {
      const next = new Set(previous);
      next.delete(id);
      return next;
    });
  }, []);

  // Deleting waits for the toast to go away, so Undo needs nothing from the backend.
  const remove = useCallback(
    (entry: HistoryEntry) => {
      setPending((previous) => new Set(previous).add(entry.id));
      let undone = false;
      const toastId = toastManager.add({
        actionProps: {
          children: "Undo",
          onClick: () => {
            undone = true;
            unhide(entry.id);
            toastManager.close(toastId);
          },
        },
        onClose: () => {
          if (undone) {
            return;
          }
          deleteHistoryEntry(entry.id).catch(() => {
            unhide(entry.id);
            toastManager.add({
              title: "Couldn't delete the dictation",
              type: "error",
            });
          });
        },
        title: "Dictation deleted",
      });
    },
    [unhide]
  );

  const groups = useMemo(
    () =>
      groupByDay(
        (history.entries ?? []).filter((e) => !pending.has(e.id)),
        new Date()
      ),
    [history.entries, pending]
  );

  const off = retention === "never" && groups.length === 0;
  const searching = query.trim() !== "";
  const nothingYet =
    history.entries?.length === 0 && !searching && search.trim() === "";

  return (
    <section aria-labelledby="history-title" className="flex flex-col gap-4">
      <div className="flex items-center justify-between gap-4">
        {/* The History tab names it on screen. */}
        <h2 className="sr-only" id="history-title">
          History
        </h2>
        {nothingYet || off ? null : (
          <InputGroup className="w-64">
            <InputGroupInput
              aria-label="Search history"
              onChange={changeSearch}
              placeholder="Search"
              type="search"
              value={search}
            />
            <InputGroupAddon>
              <Icon aria-hidden="true" icon={Search01Icon} />
            </InputGroupAddon>
          </InputGroup>
        )}
      </div>
      <p aria-live="polite" className="sr-only">
        {announcement}
      </p>
      {history.error && !history.entries ? (
        <p className="text-foreground-muted text-sm">
          Couldn't load your history.
        </p>
      ) : null}
      {history.entries || history.error ? null : <HistorySkeleton />}
      {off ? <HistoryOff /> : null}
      {nothingYet && !off ? <TeachEmpty pushToTalk={pushToTalk} /> : null}
      {history.entries && groups.length === 0 && !nothingYet ? (
        <p className="py-8 text-center text-foreground-muted text-sm">
          No dictations match “{search.trim()}”.
        </p>
      ) : null}
      {groups.map((group) => (
        <section
          aria-label={group.label}
          className="flex flex-col gap-1"
          key={group.key}
        >
          <h3 className="pb-1 font-medium text-foreground-muted text-xs">
            {group.label}
          </h3>
          <ul className="-mx-3 flex flex-col">
            {group.entries.map((entry) => (
              <HistoryRow
                entry={entry}
                key={entry.id}
                onAnnounce={setAnnouncement}
                onDelete={remove}
              />
            ))}
          </ul>
        </section>
      ))}
      {history.more ? (
        <Button
          className="self-center"
          loading={history.loadingMore}
          onClick={history.loadMore}
          type="button"
          variant="outline"
        >
          Load more
        </Button>
      ) : null}
    </section>
  );
}
