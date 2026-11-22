import type { DictationPhase, DictationState, Millis } from "@/lib/ipc";

import { useEffect, useRef, useState } from "react";

import { setPillHitRegions } from "@/lib/ipc";

import { hitRegions, SUCCESS_MS } from "./model";

/** True once `on` has stayed true for `delayMs`; false as soon as it isn't. */
export function useDelayedFlag(on: boolean, delayMs: number): boolean {
  const [flag, setFlag] = useState(false);
  useEffect(() => {
    if (!on) {
      setFlag(false);
      return;
    }
    const timer = setTimeout(() => setFlag(true), delayMs);
    return () => clearTimeout(timer);
  }, [on, delayMs]);
  return on && flag;
}

/**
 * How long the current dictation has been listening. The backend reports
 * `elapsedMs` with each state change; this keeps counting in between.
 */
export function useElapsedMs(state: DictationState): number {
  const listening = state.phase === "listening";
  const [startedAt, setStartedAt] = useState(() => Date.now());
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const at = Date.now();
    setStartedAt(at - state.elapsedMs);
    setNow(at);
  }, [state]);

  useEffect(() => {
    if (!listening) {
      return;
    }
    const timer = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(timer);
  }, [listening]);

  return listening ? Math.max(0, now - startedAt) : 0;
}

/** True for a moment after a dictation finished without a notice. */
export function useSuccessBeat(phase: DictationPhase): boolean {
  const previous = useRef(phase);
  const [success, setSuccess] = useState(false);

  useEffect(() => {
    const was = previous.current;
    previous.current = phase;
    if (was !== "processing" || phase !== "idle") {
      setSuccess(false);
      return;
    }
    setSuccess(true);
    const timer = setTimeout(() => setSuccess(false), SUCCESS_MS);
    return () => clearTimeout(timer);
  }, [phase]);

  return success;
}

/** setTimeout can't wait longer than this. */
const MAX_TIMEOUT_MS = 2_147_483_647;

/** True while `until` (from "Hide for 1 hour") is in the future. */
export function useHiddenUntil(until: Millis | null): boolean {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    setNow(Date.now());
    if (until === null) {
      return;
    }
    const wait = until - Date.now();
    if (wait <= 0) {
      return;
    }
    const timer = setTimeout(
      () => setNow(Date.now()),
      Math.min(wait + 50, MAX_TIMEOUT_MS)
    );
    return () => clearTimeout(timer);
  }, [until]);
  return until !== null && until > now;
}

/** Marks an element whose area takes clicks; everything else passes through. */
export const HIT_REGION = { "data-hit-region": "" } as const;

/** Spring animations end within this; regions are measured again after it. */
const SETTLE_MS = 450;
const THROTTLE_MS = 50;

/**
 * Reports the areas marked with {@link HIT_REGION} through
 * `set_pill_hit_regions` whenever the layout changes, so the backend can let
 * clicks anywhere else through to the app below. With `wholeWindow` (a menu
 * is open) the whole window takes clicks, so clicking beside the menu closes
 * it instead of reaching the app below.
 */
export function useHitRegions(wholeWindow: boolean): void {
  useEffect(() => {
    let last = "";
    let warned = false;
    let pending: ReturnType<typeof setTimeout> | undefined;
    let settle: ReturnType<typeof setTimeout> | undefined;

    const send = () => {
      pending = undefined;
      const bounds = { height: window.innerHeight, width: window.innerWidth };
      const rects = wholeWindow
        ? [{ ...bounds, x: 0, y: 0 }]
        : hitRegions(
            [...document.querySelectorAll("[data-hit-region]")].map((element) =>
              element.getBoundingClientRect()
            ),
            bounds
          );
      const key = JSON.stringify(rects);
      if (key === last) {
        return;
      }
      last = key;
      setPillHitRegions(rects).catch((error: unknown) => {
        if (!warned) {
          warned = true;
          console.warn("Could not report the pill's hit regions", error);
        }
      });
    };

    const schedule = () => {
      pending ??= setTimeout(send, THROTTLE_MS);
      clearTimeout(settle);
      settle = setTimeout(send, SETTLE_MS);
    };

    const resize = new ResizeObserver(schedule);
    const observeRegions = () => {
      resize.disconnect();
      for (const element of document.querySelectorAll("[data-hit-region]")) {
        resize.observe(element);
      }
    };
    const mutations = new MutationObserver(() => {
      observeRegions();
      schedule();
    });
    mutations.observe(document.body, {
      attributeFilter: ["data-hit-region"],
      attributes: true,
      childList: true,
      subtree: true,
    });
    observeRegions();
    schedule();
    window.addEventListener("resize", schedule);

    return () => {
      window.removeEventListener("resize", schedule);
      mutations.disconnect();
      resize.disconnect();
      clearTimeout(pending);
      clearTimeout(settle);
    };
  }, [wholeWindow]);
}
