import { screen, within } from "@testing-library/react";
import { App } from "./App";
import { mockTauri } from "./test/tauri";
import { renderWithTheme } from "./test/render";
import { signedInView, signedOutView } from "./test/view";

vi.mock("@tauri-apps/api/core");
vi.mock("@tauri-apps/api/event");

describe("App", () => {
  it("moves through the screens as the state changes", async () => {
    const shell = mockTauri(signedOutView());
    renderWithTheme(<App />);
    expect(await screen.findByRole("heading", { name: "Sign in to start relaying" })).toBeVisible();
    expect(shell.commands()).toEqual(["get_state"]);

    shell.emitState(
      signedOutView({
        pairing: {
          state: "waiting",
          user_code: "KQ7M-X4PD",
          verification_uri: "https://longlines.io/relay",
          verification_uri_complete: "https://longlines.io/relay/approve?code=KQ7M-X4PD",
          expires_at: new Date(Date.now() + 600_000).toISOString(),
        },
      }),
    );
    expect(screen.getByRole("heading", { name: "Finish signing in in your browser" })).toBeVisible();

    shell.emitState(signedInView());
    expect(screen.getByRole("tab", { name: "Contacts", selected: true })).toBeVisible();
    expect(screen.getByRole("button", { name: "Account menu, KK4PWJ" })).toBeVisible();

    // A revoked token: the shell reports signed out and the page drops back.
    shell.emitState(signedOutView());
    expect(screen.getByRole("heading", { name: "Sign in to start relaying" })).toBeVisible();
    expect(screen.queryByRole("tablist")).toBeNull();
  });

  it.each([
    ["signed out", signedOutView()],
    [
      "signing in",
      signedOutView({
        pairing: {
          state: "waiting",
          user_code: "KQ7M-X4PD",
          verification_uri: "https://longlines.io/relay",
          verification_uri_complete: "https://longlines.io/relay/approve?code=KQ7M-X4PD",
          expires_at: new Date(Date.now() + 600_000).toISOString(),
        },
      }),
    ],
    ["expired", signedOutView({ pairing: { state: "failed", failure: "expired" } })],
  ])("shows the title alone while %s", async (_, view) => {
    mockTauri(view);
    renderWithTheme(<App />);
    const header = await screen.findByRole("banner");
    await screen.findByRole("main");
    expect(header).toHaveTextContent(/^Long Lines Relay$/);
    expect(within(header).queryByRole("button")).toBeNull();
    expect(screen.queryByText("Connected")).toBeNull();
    expect(screen.queryByRole("tablist")).toBeNull();
  });

  it("shows progress until the shell answers", async () => {
    mockTauri(signedOutView(), { get_state: () => new Promise(() => {}) });
    renderWithTheme(<App />);
    expect(screen.getByRole("progressbar", { name: "Loading" })).toBeVisible();
  });
});
