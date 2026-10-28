import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { afterEach, vi } from "vitest";

// MUI renders can exceed findBy's 1 s default on a loaded runner.
configure({ asyncUtilTimeout: 5000 });

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});
