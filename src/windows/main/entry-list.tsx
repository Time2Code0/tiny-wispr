import { useCallback } from "react";

import { Search01Icon } from "@hugeicons/core-free-icons";

import { Icon } from "@/components/ui/icon";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group";
import { Skeleton } from "@/components/ui/skeleton";

import { Section } from "./page";

function Body({
  count,
  shown,
  error,
  loaded,
  empty,
  noun,
  search,
  children,
}: {
  children: React.ReactNode;
  count: number;
  empty: React.ReactNode;
  error: boolean;
  loaded: boolean;
  noun: string;
  search: string;
  shown: number;
}) {
  if (!loaded) {
    return error ? (
      <p className="text-foreground-muted text-sm">
        Couldn't load your {noun}.
      </p>
    ) : (
      <Skeleton aria-hidden="true" className="h-24 w-full rounded-xl" />
    );
  }
  if (count === 0) {
    return empty;
  }
  if (shown === 0) {
    return (
      <p className="py-6 text-center text-foreground-muted text-sm">
        No {noun} match “{search.trim()}”.
      </p>
    );
  }
  return (
    <ul className="flex flex-col divide-y overflow-hidden rounded-xl border">
      {children}
    </ul>
  );
}

/**
 * The saved list on Dictionary and Snippets: a title with the count, search,
 * and the loading, empty and no-match states around the rows.
 */
export function EntryList({
  title,
  noun,
  count,
  shown,
  loaded,
  error,
  search,
  onSearch,
  empty,
  children,
}: {
  children: React.ReactNode;
  count: number;
  /** Shown when there is nothing saved yet. */
  empty: React.ReactNode;
  error: boolean;
  loaded: boolean;
  /** Plural, lowercase: "words", "snippets". */
  noun: string;
  onSearch: (query: string) => void;
  search: string;
  shown: number;
  title: string;
}) {
  const change = useCallback(
    (event: React.ChangeEvent<HTMLInputElement>) =>
      onSearch(event.target.value),
    [onSearch]
  );
  return (
    <Section
      action={
        count > 0 ? (
          <InputGroup className="w-56">
            <InputGroupInput
              aria-label={`Search ${noun}`}
              onChange={change}
              placeholder="Search"
              type="search"
              value={search}
            />
            <InputGroupAddon>
              <Icon aria-hidden="true" icon={Search01Icon} />
            </InputGroupAddon>
          </InputGroup>
        ) : null
      }
      title={
        <>
          {title}{" "}
          {count > 0 ? (
            <span className="text-foreground-muted tabular-nums">{count}</span>
          ) : null}
        </>
      }
    >
      <Body
        count={count}
        empty={empty}
        error={error}
        loaded={loaded}
        noun={noun}
        search={search}
        shown={shown}
      >
        {children}
      </Body>
    </Section>
  );
}
