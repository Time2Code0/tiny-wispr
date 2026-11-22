import { useEffect, useState } from "react";

import { on } from "@/lib/ipc";

/**
 * The last `count` values of `audio:level` (0..1), oldest first. The backend
 * only sends levels while listening or testing the microphone.
 */
export function useAudioLevels(count: number): number[] {
  const [levels, setLevels] = useState<number[]>(() =>
    Array.from({ length: count }, () => 0)
  );

  useEffect(
    () =>
      on("audio:level", ({ level }) => {
        setLevels((previous) => [...previous.slice(1 - count), level]);
      }),
    [count]
  );

  return levels;
}
