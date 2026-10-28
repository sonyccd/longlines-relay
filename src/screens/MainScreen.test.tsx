import { act, screen, within } from "@testing-library/react";
import { App } from "../App";
import { mockTauri } from "../test/tauri";
import { renderWithTheme } from "../test/render";
import { feed, signedInView } from "../test/view";

vi.mock("@tauri-apps/api/core");
vi.mock("@tauri-apps/api/event");

// The fixture's last packet arrived at 14:22:02Z.
const NOW = new Date("2026-10-28T14:22:16Z");

function card(name: string): HTMLElement {
  return screen.getByRole("region", { name });
}

describe("main screen", () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime(NOW);
  });

  it("shows the account and signs out", async () => {
    const shell = mockTauri(signedInView());
    const { user } = renderWithTheme(<App />);
    const account = within(await screen.findByRole("region", { name: "Long Lines account" }));
    expect(account.getByText("Connected")).toBeVisible();
    expect(account.getByText("Signed in as")).toBeVisible();
    expect(account.getByText("KK4PWJ")).toBeVisible();
    expect(account.getByText("op@example.com")).toBeVisible();
    expect(account.getByText("Your logged contacts are posted to Long Lines as spots.")).toBeVisible();

    await user.click(account.getByRole("button", { name: "Sign out" }));
    expect(shell.commands()).toContain("sign_out");
  });

  it("holds the account's place until Long Lines answers", async () => {
    mockTauri(signedInView({ account: null }));
    renderWithTheme(<App />);
    const account = within(await screen.findByRole("region", { name: "Long Lines account" }));
    expect(account.getByText("Signed in as")).toBeVisible();
    expect(account.queryByText("KK4PWJ")).toBeNull();
  });

  it("shows the feed receiving, ticking every second", async () => {
    mockTauri(signedInView());
    renderWithTheme(<App />);
    const feedCard = within(await screen.findByRole("region", { name: "Logging feed" }));
    expect(feedCard.getByText("Receiving")).toBeVisible();
    expect(feedCard.getByText("Last contact received 14 seconds ago")).toBeVisible();
    expect(feedCard.getByText("Listening on UDP port 12060")).toBeVisible();
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(feedCard.getByText("Last contact received 15 seconds ago")).toBeVisible();
    expect(feedCard.queryByRole("alert")).toBeNull();
  });

  it("waits for logging software", async () => {
    const shell = mockTauri(
      signedInView({
        feed: feed({ status: "waiting", label: "Waiting for logging software", last_packet_at: null }),
        stream: [],
      }),
    );
    const { user } = renderWithTheme(<App />);
    const feedCard = within(await screen.findByRole("region", { name: "Logging feed" }));
    expect(feedCard.getByText("Waiting for logging software")).toBeVisible();
    expect(feedCard.getByText("Nothing received yet")).toBeVisible();
    expect(
      feedCard.getByText(
        "Turn on contact broadcasts in your logging software and send them to this computer on port 12060.",
      ),
    ).toBeVisible();
    await user.click(feedCard.getByRole("button", { name: "Set up logging software" }));
    expect(shell.commands()).toContain("open_setup_docs");

    const contacts = within(card("Contacts seen"));
    expect(contacts.getByText("Contacts you log will show up here.")).toBeVisible();
    expect(contacts.queryByRole("list")).toBeNull();
  });

  it("turns test mode on", async () => {
    const shell = mockTauri(signedInView(), {
      set_test_mode: () => signedInView({ feed: feed({ status: "test_mode", label: "Test mode", test_mode: true }) }),
    });
    const { user } = renderWithTheme(<App />);
    const feedCard = within(await screen.findByRole("region", { name: "Logging feed" }));
    const box = feedCard.getByRole("checkbox", { name: /Test mode/ });
    expect(box).not.toBeChecked();
    expect(
      feedCard.getByText(
        "Turn on test mode to make sure everything is working. Contacts show here and aren't sent to Long Lines.",
      ),
    ).toBeVisible();

    await user.click(box);
    expect(shell.calls).toContainEqual({ command: "set_test_mode", args: { on: true } });
    expect(await feedCard.findByText("Test mode is on. Contacts aren't being sent to Long Lines.")).toBeVisible();
    expect(feedCard.getByRole("checkbox", { name: /Test mode/ })).toBeChecked();
    expect(feedCard.getAllByText("Test mode").length).toBeGreaterThanOrEqual(2); // chip and label
    expect(
      within(card("Contacts seen")).getByText(
        "Everything the relay receives, newest first. In test mode it shows what would have happened.",
      ),
    ).toBeVisible();
  });

  it("warns about a callsign mismatch", async () => {
    mockTauri(
      signedInView({ feed: feed({ status: "ignoring_contacts", label: "Ignoring contacts", mismatch: "K4XYZ" }) }),
    );
    renderWithTheme(<App />);
    const feedCard = within(await screen.findByRole("region", { name: "Logging feed" }));
    expect(feedCard.getByText("Ignoring contacts")).toBeVisible();
    const alert = feedCard.getByRole("alert");
    expect(alert).toHaveTextContent("Contacts are logged as K4XYZ");
    expect(alert).toHaveTextContent(
      "This account is KK4PWJ, so contacts logged as K4XYZ are ignored. Change the callsign in your logging software, or sign in with the account for K4XYZ.",
    );
  });

  it("says the port is in use and changes it", async () => {
    const shell = mockTauri(
      signedInView({
        feed: feed({
          status: "port_in_use",
          label: "Port in use",
          listener: { state: "port_in_use", port: 12060 },
          last_packet_at: null,
        }),
      }),
    );
    const { user } = renderWithTheme(<App />);
    const feedCard = within(await screen.findByRole("region", { name: "Logging feed" }));
    expect(feedCard.getByText("Port in use")).toBeVisible();
    expect(feedCard.getByRole("alert")).toHaveTextContent("Port 12060 is in use by another program");
    expect(feedCard.queryByText("Listening on UDP port 12060")).toBeNull();

    const field = feedCard.getByRole("textbox", { name: "UDP port" });
    const change = feedCard.getByRole("button", { name: "Change port" });
    expect(change).toBeDisabled();
    await user.clear(field);
    await user.type(field, "99999");
    expect(feedCard.getByText("Enter a port from 1 to 65535.")).toBeVisible();
    expect(change).toBeDisabled();
    await user.clear(field);
    await user.type(field, "12061");
    await user.click(change);
    expect(shell.calls).toContainEqual({ command: "set_port", args: { port: 12061 } });
  });

  it("lists contacts newest first", async () => {
    mockTauri(signedInView());
    renderWithTheme(<App />);
    const contacts = within(await screen.findByRole("region", { name: "Contacts seen" }));
    expect(contacts.getByText("Everything the relay receives, newest first.")).toBeVisible();
    const rows = contacts.getAllByRole("listitem").filter((li) => li.textContent);
    expect(rows.map((r) => r.textContent)).toEqual([
      "W4ABC · 40 m CWSent as a spot14:22 UTC",
      "Couldn't read packet14:18 UTC",
      "K8QRS · 15 m CWIgnored. Logged as K4XYZ.14:09 UTC",
    ]);
  });

  it("follows state events", async () => {
    const shell = mockTauri(signedInView());
    renderWithTheme(<App />);
    await screen.findByText("W4ABC · 40 m CW");
    const next = signedInView();
    next.stream = [
      { ...next.stream[0]!, id: 4, title: "N2DEF · 20 m SSB", status: { kind: "waiting" }, status_text: "Waiting to send" },
      ...next.stream,
    ];
    shell.emitState(next);
    expect(screen.getByText("N2DEF · 20 m SSB")).toBeVisible();
    expect(screen.getByText("Waiting to send")).toBeVisible();
  });

  it("changes the settings", async () => {
    const shell = mockTauri(signedInView());
    const { user } = renderWithTheme(<App />);
    const settings = within(await screen.findByRole("region", { name: "Settings" }));
    const autostart = settings.getByRole("checkbox", { name: "Start when I log in" });
    expect(autostart).toBeChecked();
    await user.click(autostart);
    expect(shell.calls).toContainEqual({ command: "set_autostart", args: { on: false } });
    expect(settings.getByRole("textbox", { name: "UDP port" })).toHaveValue("12060");
    expect(settings.getByText("Version 0.1.0")).toBeVisible();
  });

  it("offers an update", async () => {
    const shell = mockTauri(signedInView({ update: { state: "available", version: "0.1.1" } }));
    const { user } = renderWithTheme(<App />);
    expect(await screen.findByText("Version 0.1.1 of the relay is available.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Install and restart" }));
    expect(shell.commands()).toContain("install_update");
    shell.emitState(signedInView({ update: { state: "failed", version: "0.1.1" } }));
    expect(screen.getByText("Couldn't install version 0.1.1.")).toBeVisible();
    expect(screen.getByRole("button", { name: "Try again" })).toBeVisible();
    shell.emitState(signedInView({ update: { state: "installing", version: "0.1.1" } }));
    expect(screen.getByText("Installing version 0.1.1. The relay restarts when it's done.")).toBeVisible();
  });

  it("links a .deb install to the release instead of installing", async () => {
    const url = "https://github.com/sonyccd/longlines-relay/releases/tag/v0.1.1";
    const shell = mockTauri(signedInView({ update: { state: "manual", version: "0.1.1", url } }));
    const { user } = renderWithTheme(<App />);
    expect(
      await screen.findByText("Version 0.1.1 of the relay is available. Download the new package to install it."),
    ).toBeVisible();
    expect(screen.queryByRole("button", { name: "Install and restart" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Download" }));
    expect(shell.commands()).toContain("open_update_page");
    expect(shell.commands()).not.toContain("install_update");
  });
});
