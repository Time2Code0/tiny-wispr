import type { Settings } from "@/lib/ipc";

import { useCallback, useId, useState } from "react";

import { Switch } from "@/components/ui/switch";

/** Opt in to starting at login, like other menu bar apps. Off by default. */
export function LaunchAtLogin({
  settings,
  update,
}: {
  settings: Settings;
  update: (patch: Partial<Settings>) => Promise<unknown>;
}) {
  const id = useId();
  const [pending, setPending] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);

  const change = useCallback(
    async (launchAtLogin: boolean) => {
      setPending(launchAtLogin);
      setError(null);
      try {
        await update({ launchAtLogin });
      } catch (caught) {
        setError(
          caught instanceof Error ? caught.message : "Couldn't change that"
        );
      } finally {
        setPending(null);
      }
    },
    [update]
  );

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between gap-6 rounded-xl border bg-background px-4 py-3">
        <div className="flex min-w-0 flex-col gap-0.5">
          <label className="text-foreground text-sm" htmlFor={id}>
            Launch at login
          </label>
          <p
            className="text-pretty text-foreground-muted text-xs"
            id={`${id}-description`}
          >
            Start Tiny Wispr in the background when you log in, so it's ready
            when you are.
          </p>
        </div>
        <Switch
          aria-describedby={`${id}-description`}
          checked={pending ?? settings.launchAtLogin}
          id={id}
          onCheckedChange={change}
        />
      </div>
      {error ? (
        <p className="text-destructive-foreground text-xs" role="alert">
          {error}
        </p>
      ) : null}
    </div>
  );
}
