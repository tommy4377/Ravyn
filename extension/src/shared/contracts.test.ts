import { describe, expect, it } from "vitest";
import {
  NATIVE_PROTOCOL_MIN,
  NATIVE_PROTOCOL_VERSION,
  protocolCompatible,
} from "./contracts";

describe("protocolCompatible", () => {
  it("accepts a host with the same version", () => {
    expect(
      protocolCompatible({ protocolVersion: NATIVE_PROTOCOL_VERSION }),
    ).toBe(true);
  });

  it("accepts a newer host whose window still covers our version", () => {
    expect(
      protocolCompatible({
        protocolVersion: NATIVE_PROTOCOL_VERSION + 3,
        minProtocolVersion: NATIVE_PROTOCOL_VERSION,
      }),
    ).toBe(true);
  });

  it("rejects a newer host that dropped support for our version", () => {
    expect(
      protocolCompatible({
        protocolVersion: NATIVE_PROTOCOL_VERSION + 3,
        minProtocolVersion: NATIVE_PROTOCOL_VERSION + 2,
      }),
    ).toBe(false);
  });

  it("rejects a host older than our minimum", () => {
    expect(
      protocolCompatible({ protocolVersion: NATIVE_PROTOCOL_MIN - 1 }),
    ).toBe(false);
  });

  it("treats a missing minimum as a single-version host", () => {
    expect(
      protocolCompatible({ protocolVersion: NATIVE_PROTOCOL_VERSION + 1 }),
    ).toBe(false);
  });
});
