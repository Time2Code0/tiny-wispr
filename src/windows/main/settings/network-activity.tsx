import type { NetworkRequest } from "@/lib/ipc";

import { formatBytes } from "@/components/model-row";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import { listNetworkLog } from "@/lib/ipc";

import { Section, SettingGroup } from "../page";

const SHOWN = 50;
const LOG_EVENTS = ["models:changed", "updates:state"] as const;

const WHEN = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  hour: "numeric",
  minute: "2-digit",
  month: "short",
  year: "numeric",
});

const OUTCOME: Record<NetworkRequest["result"], string> = {
  cancelled: "Paused",
  completed: "Downloaded",
  failed: "Failed",
  restarted: "Restarted",
};

/** The file a request fetched, from the last part of its URL. */
export function requestName(url: string): string {
  try {
    const { hostname, pathname } = new URL(url);
    const file = pathname.split("/").filter(Boolean).at(-1);
    return file ? decodeURIComponent(file) : hostname;
  } catch {
    return url;
  }
}

/** Newest first, at most {@link SHOWN}. */
export function latestRequests(log: NetworkRequest[]): NetworkRequest[] {
  return [...log].sort((a, b) => b.startedAt - a.startedAt).slice(0, SHOWN);
}

function countLabel(total: number): string {
  if (total > SHOWN) {
    return `The latest ${SHOWN} of ${total.toLocaleString()} requests.`;
  }
  return total === 1 ? "1 request." : `${total.toLocaleString()} requests.`;
}

function RequestRow({ request }: { request: NetworkRequest }) {
  const host = (() => {
    try {
      return new URL(request.url).hostname;
    } catch {
      return null;
    }
  })();
  return (
    <li className="flex items-start justify-between gap-4 px-4 py-3 text-sm">
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="truncate text-foreground" title={request.url}>
          {requestName(request.url)}
        </span>
        <span className="text-foreground-muted text-xs">
          <time dateTime={new Date(request.startedAt).toISOString()}>
            {WHEN.format(request.startedAt)}
          </time>
          {host ? ` · ${host}` : null}
        </span>
        {request.result === "failed" && request.message ? (
          <span className="text-pretty text-destructive-foreground text-xs">
            {request.message}
          </span>
        ) : null}
      </div>
      <div className="flex shrink-0 flex-col items-end gap-0.5 text-end">
        <span className="text-foreground tabular-nums">
          {request.bytes > 0 ? formatBytes(request.bytes) : "Nothing received"}
        </span>
        <span
          className={
            request.result === "failed"
              ? "text-destructive-foreground text-xs"
              : "text-foreground-muted text-xs"
          }
        >
          {OUTCOME[request.result]}
        </span>
      </div>
    </li>
  );
}

/** Every request the app has made, so the privacy promise can be checked. */
/** `children` (the Stay offline switch) sits above the list of requests. */
export function NetworkActivity({ children }: { children?: React.ReactNode }) {
  const log = useIpcQuery(listNetworkLog, LOG_EVENTS);
  if (!log.data) {
    return null;
  }
  const requests = latestRequests(log.data);
  return (
    <Section
      description={
        requests.length === 0
          ? "Nothing yet. Tiny Wispr only goes online when you download a model."
          : `Every request Tiny Wispr has made. ${countLabel(log.data.length)}`
      }
      title="Network activity"
    >
      {children}
      {requests.length > 0 ? (
        <SettingGroup>
          <ul aria-label="Network requests" className="flex flex-col divide-y">
            {requests.map((request) => (
              <RequestRow
                key={`${request.startedAt}-${request.url}`}
                request={request}
              />
            ))}
          </ul>
        </SettingGroup>
      ) : null}
    </Section>
  );
}
