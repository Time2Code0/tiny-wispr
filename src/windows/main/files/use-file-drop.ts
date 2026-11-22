import { useEffect, useRef, useState } from "react";

import { isTauri } from "@/lib/ipc";

/**
 * Files dropped anywhere on the window, as paths. In the app this is
 * Tauri's drag-drop event, which has real paths; in the browser preview the
 * DOM events stand in, with the file's name as its path.
 */
export function useFileDrop(onDrop: (paths: string[]) => void): boolean {
  const [dragging, setDragging] = useState(false);
  const onDropRef = useRef(onDrop);
  onDropRef.current = onDrop;

  useEffect(() => {
    if (isTauri) {
      let disposed = false;
      let unlisten: (() => void) | undefined;
      import("@tauri-apps/api/webview")
        .then(({ getCurrentWebview }) =>
          getCurrentWebview().onDragDropEvent(({ payload }) => {
            if (payload.type === "enter" || payload.type === "over") {
              setDragging(true);
            } else if (payload.type === "leave") {
              setDragging(false);
            } else {
              setDragging(false);
              onDropRef.current(payload.paths);
            }
          })
        )
        .then((fn) => {
          if (disposed) {
            fn();
          } else {
            unlisten = fn;
          }
        })
        .catch((error: unknown) => {
          console.error("Could not listen for dropped files", error);
        });
      return () => {
        disposed = true;
        unlisten?.();
      };
    }

    // `dragleave` fires when moving between children, so count the depth.
    let depth = 0;
    const hasFiles = (event: DragEvent) =>
      event.dataTransfer?.types.includes("Files") ?? false;
    const enter = (event: DragEvent) => {
      if (hasFiles(event)) {
        depth += 1;
        setDragging(true);
      }
    };
    const over = (event: DragEvent) => {
      if (hasFiles(event)) {
        event.preventDefault();
      }
    };
    const leave = () => {
      depth = Math.max(0, depth - 1);
      if (depth === 0) {
        setDragging(false);
      }
    };
    const drop = (event: DragEvent) => {
      if (!hasFiles(event)) {
        return;
      }
      event.preventDefault();
      depth = 0;
      setDragging(false);
      const names = Array.from(event.dataTransfer?.files ?? [], (f) => f.name);
      onDropRef.current(names);
    };
    window.addEventListener("dragenter", enter);
    window.addEventListener("dragover", over);
    window.addEventListener("dragleave", leave);
    window.addEventListener("drop", drop);
    return () => {
      window.removeEventListener("dragenter", enter);
      window.removeEventListener("dragover", over);
      window.removeEventListener("dragleave", leave);
      window.removeEventListener("drop", drop);
    };
  }, []);

  return dragging;
}
