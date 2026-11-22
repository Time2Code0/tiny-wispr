import type { UnlistenFn } from "@tauri-apps/api/event";
import type { Backend, CommandArgs, CommandName, CommandResult } from "./types";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { IpcError } from "./errors";

/** The real backend: Tauri commands and events. */
export function createTauriBackend(): Backend {
  return {
    invoke<C extends CommandName>(
      command: C,
      args: CommandArgs<C>
    ): Promise<CommandResult<C>> {
      return invoke<CommandResult<C>>(command, args ?? {}).catch(
        (error: unknown) => {
          throw IpcError.from(error);
        }
      );
    },
    kind: "tauri",
    listen(event, handler) {
      let disposed = false;
      let unlisten: UnlistenFn | undefined;
      listen<Parameters<typeof handler>[0]>(event, (e) =>
        handler(e.payload)
      ).then(
        (fn) => {
          if (disposed) {
            fn();
          } else {
            unlisten = fn;
          }
        },
        (error: unknown) => {
          console.error(`Could not listen to ${event}`, error);
        }
      );
      return () => {
        disposed = true;
        unlisten?.();
      };
    },
  };
}
