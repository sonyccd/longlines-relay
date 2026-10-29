import { screen, within } from "@testing-library/react";
import { App } from "../App";
import type { ShellView } from "../lib/types";
import { mockTauri } from "../test/tauri";
import { renderWithTheme } from "../test/render";
import { cluster, signedInView } from "../test/view";

vi.mock("@tauri-apps/api/core");
vi.mock("@tauri-apps/api/event");

/** Only other computers on this computer: loopback, no network address. */
function loopbackView(): ShellView {
  return signedInView({
    cluster: cluster({ lan: false, addresses: { local: "127.0.0.1", lan: null } }),
    settings: { port: 12060, autostart: true, cluster_enabled: true, cluster_port: 7373, cluster_lan: false },
  });
}

async function openSetup(view: ShellView, answers: Record<string, (args: unknown) => unknown> = {}) {
  const shell = mockTauri(view, answers);
  const { user } = renderWithTheme(<App />);
  await user.click(await screen.findByRole("tab", { name: "DX cluster" }));
  await user.click(screen.getByRole("button", { name: "Set up logging software" }));
  return { shell, user };
}

function addressCard() {
  return within(screen.getByRole("region", { name: "Cluster address" }));
}

function steps() {
  return within(screen.getByRole("region", { name: "Steps" }));
}

describe("DX cluster setup screen", () => {
  it("replaces the tabs, with a back arrow, and shows the N1MM Logger+ steps", async () => {
    await openSetup(signedInView());
    const header = within(screen.getByRole("banner"));
    expect(header.getByRole("heading", { level: 1, name: "Set up logging software" })).toBeVisible();
    expect(header.getByRole("button", { name: "Back" })).toHaveFocus();
    expect(screen.queryByRole("tablist", { name: "Relay" })).toBeNull();
    expect(screen.queryByRole("button", { name: /^Account menu/ })).toBeNull();
    expect(
      screen.getByText(
        "Your logging software connects to the relay as if it were a DX cluster. Matched spots then show up in its bandmap.",
      ),
    ).toBeVisible();

    const software = within(screen.getByRole("tablist", { name: "Logging software" }));
    expect(software.getAllByRole("tab").map((t) => t.textContent)).toEqual(["N1MM Logger+", "Other software"]);
    expect(software.getByRole("tab", { name: "N1MM Logger+" })).toHaveAttribute("aria-selected", "true");

    const card = addressCard();
    expect(card.getByText("127.0.0.1")).toBeVisible();
    expect(card.getByText("7373")).toBeVisible();
    expect(card.getByText("Callsign, if asked")).toBeVisible();
    expect(card.getByText("KK4PWJ")).toBeVisible();
    expect(card.getByRole("button", { name: "Copy address 127.0.0.1" })).toBeVisible();
    expect(card.getByRole("button", { name: "Copy port 7373" })).toBeVisible();
    expect(card.getByRole("button", { name: "Copy callsign KK4PWJ" })).toBeVisible();

    const list = steps();
    expect(list.getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "1Open the Telnet window in N1MM Logger+.",
      "2Add a new cluster with the address and port above.",
      "3Connect to it.Other computers on your network use 192.168.1.42.",
      "4Check that it works.Send a test spot. It should show up in the bandmap.Send test spot",
    ]);
    expect(screen.queryByText(/won't connect/)).toBeNull();
    expect(
      screen.getByText(
        "N1MM Logger+ applies its own spot filters and band limits. If a spot doesn't show up, check those first.",
      ),
    ).toBeVisible();
  });

  it("shows the network address only while other computers may connect", async () => {
    await openSetup(signedInView());
    expect(addressCard().getByText("Other computers")).toBeVisible();
    expect(addressCard().getByRole("button", { name: "Copy address 192.168.1.42" })).toBeVisible();
  });

  it("leaves the network address out while only this computer may connect", async () => {
    await openSetup(loopbackView());
    expect(screen.queryByText(/192\.168\.1\.42/)).toBeNull();
    expect(addressCard().queryByText("Other computers")).toBeNull();
    expect(steps().getAllByRole("listitem")[2]).toHaveTextContent(/^3Connect to it\.$/);
  });

  it("shows the steps for other software", async () => {
    const { user } = await openSetup(signedInView());
    await user.click(screen.getByRole("tab", { name: "Other software" }));
    expect(screen.getByRole("tab", { name: "Other software" })).toHaveAttribute("aria-selected", "true");
    expect(steps().getAllByRole("listitem")[0]).toHaveTextContent(
      "Open the DX cluster or Telnet settings in your logging software.",
    );
    expect(
      screen.getByText(
        "Your logging software applies its own spot filters and band limits. If a spot doesn't show up, check those first.",
      ),
    ).toBeVisible();
    // The address card stays the same.
    expect(addressCard().getByText("127.0.0.1")).toBeVisible();
  });

  it("sends a test spot and says so", async () => {
    const { shell, user } = await openSetup(signedInView());
    await user.click(steps().getByRole("button", { name: "Send test spot" }));
    expect(shell.commands()).toContain("send_test_spot");
    expect(await steps().findByText("Test spot sent.")).toBeVisible();
  });

  it("says when the test spot couldn't be sent", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const { user } = await openSetup(signedInView(), {
      send_test_spot: () => Promise.reject(new Error("The DX cluster isn't listening.")),
    });
    await user.click(steps().getByRole("button", { name: "Send test spot" }));
    expect(await steps().findByText("Couldn't send the test spot. Check that the DX cluster is on.")).toBeVisible();
    expect(error).toHaveBeenCalled();
  });

  it("can't send a test spot while nothing listens", async () => {
    await openSetup(
      signedInView({
        cluster: cluster({
          status: "port_in_use",
          label: "Port in use",
          listener: { state: "port_in_use", port: 7373 },
        }),
      }),
    );
    expect(steps().getByRole("button", { name: "Send test spot" })).toBeDisabled();
  });

  it("goes back to the DX cluster tab", async () => {
    const { user } = await openSetup(signedInView());
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByRole("tab", { name: "DX cluster", selected: true })).toBeVisible();
    expect(screen.getByRole("region", { name: "Local DX cluster" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Account menu, KK4PWJ" })).toBeVisible();
  });

  it("leaves the setup screen on sign-out", async () => {
    const shell = mockTauri(signedInView());
    const { user } = renderWithTheme(<App />);
    await user.click(await screen.findByRole("tab", { name: "DX cluster" }));
    await user.click(screen.getByRole("button", { name: "Set up logging software" }));
    expect(screen.getByRole("heading", { level: 1, name: "Set up logging software" })).toBeVisible();
    shell.emitState(signedInView({ signed_in: false, account: null, upstream: { state: "signed_out" } }));
    expect(screen.getByRole("heading", { name: "Sign in to start relaying" })).toBeVisible();
  });
});
