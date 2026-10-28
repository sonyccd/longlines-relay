import { parsePort } from "./port";

describe("parsePort", () => {
  it("accepts 1 to 65535", () => {
    expect(parsePort("12060")).toBe(12060);
    expect(parsePort(" 1 ")).toBe(1);
    expect(parsePort("65535")).toBe(65535);
  });
  it("refuses anything else", () => {
    for (const text of ["", "0", "65536", "-1", "12.5", "1e3", "abc", "123456"]) {
      expect(parsePort(text)).toBeNull();
    }
  });
});
