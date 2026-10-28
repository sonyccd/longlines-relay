import { ago, countdown, withoutScheme } from "./time";

const at = (s: number) => new Date(Date.UTC(2026, 9, 28, 14, 0, 0) + s * 1000);

describe("ago", () => {
  it("counts seconds, minutes, hours and days", () => {
    expect(ago(at(0), at(14))).toBe("14 seconds ago");
    expect(ago(at(0), at(1))).toBe("1 second ago");
    expect(ago(at(0), at(0))).toBe("1 second ago");
    expect(ago(at(5), at(0))).toBe("1 second ago");
    expect(ago(at(0), at(59.9))).toBe("59 seconds ago");
    expect(ago(at(0), at(60))).toBe("1 minute ago");
    expect(ago(at(0), at(185))).toBe("3 minutes ago");
    expect(ago(at(0), at(3600))).toBe("1 hour ago");
    expect(ago(at(0), at(7300))).toBe("2 hours ago");
    expect(ago(at(0), at(86400 * 5))).toBe("5 days ago");
  });
});

describe("countdown", () => {
  it("shows minutes and seconds left", () => {
    expect(countdown(at(600), at(19))).toBe("9:41");
    expect(countdown(at(600), at(0))).toBe("10:00");
    expect(countdown(at(600), at(595))).toBe("0:05");
    expect(countdown(at(600), at(599.5))).toBe("0:01");
    expect(countdown(at(600), at(700))).toBe("0:00");
  });
});

describe("withoutScheme", () => {
  it("drops the scheme and a trailing slash", () => {
    expect(withoutScheme("https://longlines.io/relay")).toBe("longlines.io/relay");
    expect(withoutScheme("http://127.0.0.1:5173/relay/")).toBe("127.0.0.1:5173/relay");
    expect(withoutScheme("longlines.io/relay")).toBe("longlines.io/relay");
  });
});
