import { act, screen, within } from "@testing-library/react";
import { App } from "../App";
import type { ShellView } from "../lib/types";
import { mockTauri } from "../test/tauri";
import { renderWithTheme } from "../test/render";
import { SNAPSHOT, cluster, signedInView } from "../test/view";

vi.mock("@tauri-apps/api/core");
vi.mock("@tauri-apps/api/event");

const NOW = new Date("2026-10-28T14:22:16Z");
const SETTINGS = { port: 12060, autostart: true, cluster_enabled: true, cluster_port: 7373, cluster_lan: false };

/** The cluster switched off, as on a fresh install. */
function offView(): ShellView {
  return signedInView({
    cluster: cluster({
      enabled: false,
      status: "off",
      label: "Off",
      listener: { state: "stopped", port: 7373 },
      lan: false,
      addresses: { local: "127.0.0.1", lan: null },
      last_sent_at: null,
      sent_today: 0,
      clients: 0,
      upstream: { state: "off" },
    }),
    sent: [],
    settings: { ...SETTINGS, cluster_enabled: false },
  });
}

/** On, the stream connected, one program connected, nothing sent yet (RelayDxListening). */
function listeningView(overrides: Partial<ShellView["cluster"]> = {}): ShellView {
  return signedInView({
    cluster: cluster({
      status: "listening",
      label: "Listening",
      lan: false,
      addresses: { local: "127.0.0.1", lan: null },
      last_sent_at: null,
      sent_today: 0,
      clients: 1,
      ...overrides,
    }),
    sent: [],
    settings: SETTINGS,
  });
}

/** Spots flowing, other computers allowed (RelayDxRunning). */
function runningView(): ShellView {
  return signedInView({
    cluster: cluster({
      last_sent_at: new Date(NOW.getTime() - 9000).toISOString(),
      sent_today: 47,
      clients: 3,
    }),
  });
}

async function openDxTab(view: ShellView, answers: Record<string, (args: unknown) => unknown> = {}) {
  const shell = mockTauri(view, answers);
  const { user } = renderWithTheme(<App />);
  await user.click(await screen.findByRole("tab", { name: "DX cluster" }));
  const card = within(screen.getByRole("region", { name: "Local DX cluster" }));
  return { shell, user, card };
}

describe("DX cluster tab", () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime(NOW);
  });

  it("says what the cluster is while it's off, and turns it on", async () => {
    const { shell, user, card } = await openDxTab(offView(), { set_cluster_enabled: () => listeningView() });
    const toggle = card.getByRole("switch", { name: "Local DX cluster" });
    expect(toggle).not.toBeChecked();
    expect(card.getByText("Off")).toBeVisible();
    expect(
      card.getByText(
        "Turn this on to show Long Lines spots in your logging software's bandmap. Your logging software connects to this computer as if it were a DX cluster.",
      ),
    ).toBeVisible();
    expect(card.getByText("Nothing listens on this computer while it's off.")).toBeVisible();
    expect(card.queryByRole("group", { name: "Connection details for your logging software" })).toBeNull();
    expect(card.queryByRole("button")).toBeNull();
    expect(screen.queryByRole("region", { name: "Spots sent" })).toBeNull();

    await user.click(toggle);
    expect(shell.calls).toContainEqual({ command: "set_cluster_enabled", args: { on: true } });
    expect(await card.findByText("Listening")).toBeVisible();
    expect(card.getByRole("switch", { name: "Local DX cluster" })).toBeChecked();
  });

  it("listens with no spots yet and links to adding a destination", async () => {
    const { shell, user, card } = await openDxTab(listeningView());
    expect(card.getByText("Listening")).toBeVisible();
    expect(card.getByText("No spots sent yet")).toBeVisible();
    expect(card.getByText("1 program connected")).toBeVisible();
    expect(card.getByRole("status")).toHaveTextContent(
      "No destination sends spots to this computer yet. In Long Lines, add a Local DX cluster destination and choose this relay.",
    );

    const grid = within(card.getByRole("group", { name: "Connection details for your logging software" }));
    expect(grid.getByText("This computer")).toBeVisible();
    expect(grid.queryByText("Other computers")).toBeNull();
    expect(grid.getByRole("button", { name: "Copy address 127.0.0.1" })).toBeVisible();
    expect(grid.getByRole("button", { name: "Copy port 7373" })).toBeVisible();

    const lan = card.getByRole("checkbox", { name: "Let other computers on this network connect" });
    expect(lan).not.toBeChecked();
    expect(lan).toHaveAccessibleDescription("Anyone on your network can connect while this is on.");

    const buttons = card.getAllByRole("button").filter((b) => !b.getAttribute("aria-label")?.startsWith("Copy"));
    expect(buttons.map((b) => b.textContent)).toEqual([
      "Add destination",
      "Set up logging software",
      "Send test spot",
      "Change port",
    ]);
    await user.click(card.getByRole("button", { name: "Add destination" }));
    expect(shell.commands()).toContain("open_add_destination");

    await user.click(lan);
    expect(shell.calls).toContainEqual({ command: "set_cluster_lan", args: { on: true } });
    expect(screen.queryByRole("region", { name: "Spots sent" })).toBeNull();
  });

  it("keeps the destination note through test spots", async () => {
    const view = listeningView({ status: "running", label: "Running", last_sent_at: "2026-10-28T14:02:00Z" });
    view.sent = SNAPSHOT.sent.filter((row) => row.test);
    const { card } = await openDxTab(view);
    expect(card.getByText(/^No destination sends spots/)).toBeVisible();
    expect(card.getByRole("button", { name: "Add destination" })).toBeVisible();
  });

  it("shows spots flowing, newest first, with the test spot tagged", async () => {
    const { shell, user, card } = await openDxTab(runningView());
    expect(card.getByText("Running")).toBeVisible();
    expect(card.getByText("Last spot sent 9 seconds ago · 47 spots today")).toBeVisible();
    expect(card.getByText("3 programs connected")).toBeVisible();
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(card.getByText("Last spot sent 10 seconds ago · 47 spots today")).toBeVisible();
    expect(card.queryByText(/^No destination sends spots/)).toBeNull();
    expect(card.queryByRole("button", { name: "Add destination" })).toBeNull();

    const grid = within(card.getByRole("group", { name: "Connection details for your logging software" }));
    expect(grid.getByText("Other computers")).toBeVisible();
    expect(grid.getByRole("button", { name: "Copy address 192.168.1.42" })).toBeVisible();
    expect(grid.getAllByRole("button", { name: "Copy port 7373" })).toHaveLength(2);
    expect(card.getByRole("checkbox", { name: "Let other computers on this network connect" })).toBeChecked();

    const sent = within(screen.getByRole("region", { name: "Spots sent" }));
    expect(sent.getByText("What the relay sent to your logging software, newest first.")).toBeVisible();
    expect(
      sent
        .getAllByRole("listitem")
        .filter((li) => li.textContent)
        .map((li) => li.textContent),
    ).toEqual([
      "K4ABC · 14062.0 kHz CWPOTA K-0039 · from POTA states, CW14:14 UTC",
      "LLTEST · 14050.0 kHz CWTest spot sent from this relay14:02 UTCTest",
    ]);

    await user.click(card.getByRole("button", { name: "Send test spot" }));
    expect(shell.commands()).toContain("send_test_spot");

    await user.click(card.getByRole("switch", { name: "Local DX cluster" }));
    expect(shell.calls).toContainEqual({ command: "set_cluster_enabled", args: { on: false } });
  });

  it("says no programs are connected", async () => {
    const { card } = await openDxTab(listeningView({ clients: 0 }));
    expect(card.getByText("No programs connected")).toBeVisible();
  });

  it("copies the address", async () => {
    const { user, card } = await openDxTab(runningView());
    await user.click(card.getByRole("button", { name: "Copy address 192.168.1.42" }));
    expect(await navigator.clipboard.readText()).toBe("192.168.1.42");
    expect(await screen.findByRole("tooltip")).toHaveTextContent("Copied");
  });

  it("says the port is in use and changes it", async () => {
    const { shell, user, card } = await openDxTab(
      listeningView({
        status: "port_in_use",
        label: "Port in use",
        listener: { state: "port_in_use", port: 7373 },
        clients: 0,
      }),
    );
    expect(card.getByText("Port in use")).toBeVisible();
    const alert = within(card.getByRole("alert"));
    expect(alert.getByText("Port 7373 is in use by another program")).toBeVisible();
    expect(alert.getByText("Choose another port here and in your logging software.")).toBeVisible();
    expect(card.queryByRole("group", { name: "Connection details for your logging software" })).toBeNull();
    expect(card.queryByText(/^No destination sends spots/)).toBeNull();
    expect(card.getByRole("button", { name: "Send test spot" })).toBeDisabled();
    // The field is in the alert, so the card has no second Change port.
    expect(card.getAllByRole("button", { name: "Change port" })).toHaveLength(1);

    const field = alert.getByRole("textbox", { name: "Port" });
    await user.clear(field);
    await user.type(field, "7380");
    await user.click(alert.getByRole("button", { name: "Change port" }));
    expect(shell.calls).toContainEqual({ command: "set_cluster_port", args: { port: 7380 } });
  });

  it("says when the default port was taken and the relay moved on", async () => {
    const { card } = await openDxTab(listeningView({ listener: { state: "listening", port: 7374 }, port: 7374 }));
    expect(
      card.getByText(
        "Port 7373 is in use by another program, so the relay listens on port 7374. Use port 7374 in your logging software.",
      ),
    ).toBeVisible();
    expect(card.getByRole("button", { name: "Copy port 7374" })).toBeVisible();
  });

  it("changes the port", async () => {
    const { shell, user, card } = await openDxTab(listeningView());
    expect(card.queryByRole("textbox", { name: "Port" })).toBeNull();
    const toggle = card.getByRole("button", { name: "Change port" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const field = card.getByRole("textbox", { name: "Port" });
    expect(field).toHaveValue("7373");
    await user.clear(field);
    await user.type(field, "7400");
    const buttons = card.getAllByRole("button", { name: "Change port" });
    await user.click(buttons[buttons.length - 1]!);
    expect(shell.calls).toContainEqual({ command: "set_cluster_port", args: { port: 7400 } });
    expect(card.queryByRole("textbox", { name: "Port" })).toBeNull();
  });

  it("asks to sign in again when the token can't receive spots", async () => {
    const { shell, user, card } = await openDxTab(
      listeningView({ status: "scope_missing", label: "Sign in again", upstream: { state: "off" } }),
    );
    expect(card.getAllByText("Sign in again")[0]).toBeVisible(); // the chip
    const alert = within(card.getByRole("alert"));
    expect(alert.getByText("Sign in again to use the DX cluster")).toBeVisible();
    expect(
      alert.getByText(
        "This computer was signed in before the relay could receive spots. Sign in again and select Allow in your browser.",
      ),
    ).toBeVisible();
    expect(card.queryByText(/^No destination sends spots/)).toBeNull();
    // The cluster keeps listening for logging software meanwhile.
    expect(card.getByRole("group", { name: "Connection details for your logging software" })).toBeVisible();

    await user.click(alert.getByRole("button", { name: "Sign in again" }));
    expect(shell.commands().filter((c) => c === "sign_out" || c === "start_sign_in")).toEqual([
      "sign_out",
      "start_sign_in",
    ]);
  });

  it("says when the spots stream is reconnecting", async () => {
    const at = new Date(NOW.getTime() + 42_000).toISOString();
    const { card } = await openDxTab(listeningView({ upstream: { state: "retrying", at } }));
    expect(card.getByText("Couldn't reach Long Lines for spots. Trying again in 0:42.")).toBeVisible();
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(card.getByText("Couldn't reach Long Lines for spots. Trying again in 0:41.")).toBeVisible();
    // Whether a destination exists can't be told while the stream is down.
    expect(card.queryByText(/^No destination sends spots/)).toBeNull();
  });
});
