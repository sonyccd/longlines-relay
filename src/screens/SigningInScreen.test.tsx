import { act, screen } from "@testing-library/react";
import { App } from "../App";
import { mockTauri } from "../test/tauri";
import { renderWithTheme } from "../test/render";
import { signedOutView } from "../test/view";

vi.mock("@tauri-apps/api/core");
vi.mock("@tauri-apps/api/event");

const NOW = new Date("2026-10-28T14:20:19Z");

const waiting = signedOutView({
  pairing: {
    state: "waiting",
    user_code: "KQ7M-X4PD",
    verification_uri: "https://longlines.io/relay",
    verification_uri_complete: "https://longlines.io/relay/approve?code=KQ7M-X4PD",
    expires_at: "2026-10-28T14:30:00Z",
  },
});

describe("signing in", () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime(NOW);
  });

  it("shows the code and counts down", async () => {
    mockTauri(waiting);
    renderWithTheme(<App />);

    expect(await screen.findByRole("heading", { level: 2, name: "Finish signing in in your browser" })).toBeVisible();
    expect(
      screen.getByText("Long Lines opened in your browser. Check that it shows this code, then select Allow."),
    ).toBeVisible();
    expect(screen.getByText("Your code")).toBeVisible();
    expect(screen.getByTestId("user-code")).toHaveTextContent("KQ7M-X4PD");
    expect(screen.getByText("Expires in 9:41")).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent("Waiting for you to approve");

    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(screen.getByText("Expires in 9:39")).toBeVisible();
  });

  it("cancels, and opens the browser again", async () => {
    const shell = mockTauri(waiting);
    const { user } = renderWithTheme(<App />);

    await user.click(await screen.findByRole("button", { name: "Open browser again" }));
    expect(shell.commands()).toContain("open_sign_in_page");

    const footer = screen.getByText(/Browser didn't open\? Go to/);
    expect(footer).toHaveTextContent("Browser didn't open? Go to longlines.io/relay and enter the code.");
    await user.click(screen.getByRole("button", { name: "longlines.io/relay" }));
    expect(shell.commands().filter((c) => c === "open_sign_in_page")).toHaveLength(2);

    shell.view = signedOutView();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(shell.commands()).toContain("cancel_sign_in");
    expect(await screen.findByRole("heading", { name: "Sign in to start relaying" })).toBeVisible();
  });
});
