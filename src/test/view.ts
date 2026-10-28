import snapshot from "./fixtures/snapshot.json";
import type { FeedView, ShellView, Snapshot } from "../lib/types";

/** The core's snapshot fixture (written by crates/relay-core/tests/ui_shape.rs). */
export const SNAPSHOT: Snapshot = snapshot as Snapshot;

/** A signed-in view: the fixture's account, feed and rows, plus the shell's own fields. */
export function signedInView(overrides: Partial<ShellView> = {}): ShellView {
  return {
    ...structuredClone(SNAPSHOT),
    upstream: { state: "connected" },
    signed_in: true,
    settings: { port: 12060, autostart: true },
    update: { state: "none" },
    version: "0.1.0",
    ...overrides,
  };
}

/** A signed-out view with nothing received. */
export function signedOutView(overrides: Partial<ShellView> = {}): ShellView {
  return signedInView({
    account: null,
    upstream: { state: "signed_out" },
    signed_in: false,
    stream: [],
    ...overrides,
  });
}

export function feed(overrides: Partial<FeedView>): FeedView {
  return { ...structuredClone(SNAPSHOT.feed), ...overrides };
}
