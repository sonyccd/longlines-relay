import { screen } from "@testing-library/react";
import { App } from "../App";
import { mockTauri } from "../test/tauri";
import { renderWithTheme } from "../test/render";
import { signedOutView } from "../test/view";

vi.mock("@tauri-apps/api/core");
vi.mock("@tauri-apps/api/event");

describe("signed out", () => {
  it("says what the relay does and signs in", async () => {
    const shell = mockTauri(
      signedOutView({
        settings: { port: 12061, autostart: true, cluster_enabled: false, cluster_port: 7373, cluster_lan: false },
      }),
    );
    const { user } = renderWithTheme(<App />);

    expect(await screen.findByRole("heading", { level: 2, name: "Sign in to start relaying" })).toBeVisible();
    expect(
      screen.getByText(
        "The relay sends the contacts you log to Long Lines as spots. Sign in with your Long Lines account to connect this computer.",
      ),
    ).toBeVisible();
    expect(screen.getByRole("heading", { level: 1, name: "Long Lines Relay" })).toBeVisible();
    const steps = screen.getAllByRole("listitem");
    expect(steps.map((s) => s.textContent)).toEqual([
      "1Sign in with Long LinesYour browser opens so you can approve this computer. You don't type a password here.",
      "2Set up your logging softwarePoint its contact broadcast at this computer, port 12061. See how",
      "3Log contacts as usualEach contact shows up here and on Long Lines a moment later.",
    ]);

    await user.click(screen.getByRole("button", { name: "Sign in with Long Lines" }));
    expect(shell.commands()).toContain("start_sign_in");

    await user.click(screen.getByRole("button", { name: "Create one on Long Lines" }));
    expect(shell.commands()).toContain("open_sign_up");
    expect(screen.getByText(/No account\?/)).toBeVisible();

    await user.click(screen.getByRole("button", { name: "See how" }));
    expect(shell.commands()).toContain("open_setup_docs");
  });

  it("shows why the last sign-in stopped", async () => {
    mockTauri(signedOutView({ pairing: { state: "failed", failure: "expired" } }));
    renderWithTheme(<App />);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("The code expired");
    expect(alert).toHaveTextContent("It wasn't approved within 10 minutes. Sign in again to get a new code.");
    // The expired mockup drops the steps and the sign-up line.
    expect(screen.queryByRole("listitem")).toBeNull();
    expect(screen.queryByText(/No account\?/)).toBeNull();
    expect(screen.getByRole("button", { name: "Sign in with Long Lines" })).toBeEnabled();
  });

  it.each([
    ["denied", "Sign-in was denied"],
    ["unreachable", "Couldn't reach Long Lines"],
    ["refused", "Long Lines didn't accept the sign-in"],
  ] as const)("words a %s sign-in", async (failure, title) => {
    mockTauri(signedOutView({ pairing: { state: "failed", failure } }));
    renderWithTheme(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent(title);
  });

  it("disables the button while asking for a code", async () => {
    mockTauri(signedOutView({ pairing: { state: "starting" } }));
    renderWithTheme(<App />);
    expect(await screen.findByRole("button", { name: "Sign in with Long Lines" })).toBeDisabled();
  });
});
