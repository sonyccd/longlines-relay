import { noDestinationYet } from "./cluster";
import { SNAPSHOT, cluster } from "../test/view";

const TEST_ONLY = SNAPSHOT.sent.filter((row) => row.test);

describe("noDestinationYet", () => {
  it("holds while the stream is connected and only test spots have been sent", () => {
    expect(noDestinationYet(cluster({ status: "listening" }), [])).toBe(true);
    expect(noDestinationYet(cluster({ status: "running" }), TEST_ONLY)).toBe(true);
  });

  it("stops once a spot from Long Lines has been sent", () => {
    expect(noDestinationYet(cluster({ status: "running" }), SNAPSHOT.sent)).toBe(false);
  });

  it("says nothing while the stream isn't connected", () => {
    for (const upstream of [
      { state: "off" },
      { state: "connecting" },
      { state: "retrying", at: "2026-10-28T14:23:00Z" },
    ] as const) {
      expect(noDestinationYet(cluster({ status: "listening", upstream }), [])).toBe(false);
    }
  });

  it("says nothing while off, with the port in use or the scope missing", () => {
    for (const status of ["off", "port_in_use", "scope_missing"] as const) {
      expect(noDestinationYet(cluster({ status }), [])).toBe(false);
    }
  });
});
