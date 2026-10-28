// A stand-in for the Rust shell: `invoke` answers from a table and records every call, and
// `emitState` delivers a `state` event to the page's listener. Tests call `mockTauri()` after
// `vi.mock("@tauri-apps/api/core")` and `vi.mock("@tauri-apps/api/event")`.

import { invoke } from "@tauri-apps/api/core";
import { listen, type Event, type EventCallback } from "@tauri-apps/api/event";
import { act } from "@testing-library/react";
import { vi } from "vitest";
import type { ShellView } from "../lib/types";

export interface Call {
  command: string;
  args: unknown;
}

export interface FakeShell {
  calls: Call[];
  /** Commands called, in order. */
  commands: () => string[];
  /** Sends a `state` event, as the shell does after every change. */
  emitState: (view: ShellView) => void;
  /** What `get_state` answers; also what commands answer unless `answers` says otherwise. */
  view: ShellView;
}

export function mockTauri(
  view: ShellView,
  answers: Record<string, (args: unknown) => unknown> = {},
): FakeShell {
  const calls: Call[] = [];
  let handler: EventCallback<ShellView> | undefined;
  const shell: FakeShell = {
    calls,
    commands: () => calls.map((c) => c.command),
    view,
    emitState: (next) => {
      act(() => {
        handler?.({ event: "state", id: 1, payload: next } as Event<ShellView>);
      });
    },
  };
  vi.mocked(invoke).mockImplementation((command: string, args?: unknown) => {
    calls.push({ command, args });
    const answer = answers[command];
    if (answer) return Promise.resolve(answer(args));
    if (command.startsWith("open_") || command === "install_update") return Promise.resolve(undefined);
    return Promise.resolve(shell.view);
  });
  vi.mocked(listen).mockImplementation((event: string, callback: EventCallback<unknown>) => {
    if (event === "state") handler = callback as EventCallback<ShellView>;
    return Promise.resolve(() => {
      handler = undefined;
    });
  });
  return shell;
}
