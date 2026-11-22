import type { CommandError, ErrorKind } from "./types";

/** A rejected command. Both backends reject with this. */
export class IpcError extends Error implements CommandError {
  readonly kind: ErrorKind;

  constructor(kind: ErrorKind, message: string) {
    super(message);
    this.name = "IpcError";
    this.kind = kind;
  }

  /** Wraps whatever Tauri rejected with (`{ kind, message }` or a string). */
  static from(error: unknown): IpcError {
    if (error instanceof IpcError) {
      return error;
    }
    if (
      typeof error === "object" &&
      error !== null &&
      "kind" in error &&
      "message" in error &&
      typeof error.message === "string"
    ) {
      return new IpcError(error.kind as ErrorKind, error.message);
    }
    return new IpcError("internal", String(error));
  }
}

export function isNotImplemented(error: unknown): boolean {
  return error instanceof IpcError && error.kind === "notImplemented";
}
