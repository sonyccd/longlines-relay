import { useEffect, useState } from "react";
import { relay } from "./relay";
import type { ShellView } from "./types";

/**
 * The shell's state: read once, then kept current by the `state` event. Commands that answer
 * with a view also set it, so the page never waits for the event.
 */
export function useRelay(): [ShellView | null, (view: ShellView) => void] {
  const [view, setView] = useState<ShellView | null>(null);
  useEffect(() => {
    let live = true;
    let unlisten: (() => void) | undefined;
    void relay
      .onState((next) => {
        if (live) setView(next);
      })
      .then((stop) => {
        if (live) unlisten = stop;
        else stop();
      });
    void relay.getState().then((first) => {
      // An event may have arrived first; it is at least as new.
      if (live) setView((current) => current ?? first);
    });
    return () => {
      live = false;
      unlisten?.();
    };
  }, []);
  return [view, setView];
}
