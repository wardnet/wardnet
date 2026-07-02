import { describe, expect, it } from "vitest";
import * as sdk from "../../src/lib/sdk";

describe("sdk singletons", () => {
  it("constructs a shared client and one instance per service", () => {
    expect(sdk.client).toBeDefined();
    for (const name of [
      "authService",
      "deviceService",
      "tunnelService",
      "providerService",
      "systemService",
      "networkService",
      "setupService",
      "infoService",
      "dhcpService",
      "dnsService",
      "dnsFilterService",
      "dnsLocalService",
      "dnsLogStreamService",
      "jobsService",
      "logService",
      "statsService",
      "updateService",
      "backupService",
      "remoteAccessService",
      "ruleRequestService",
    ] as const) {
      expect(sdk[name], name).toBeDefined();
    }
  });
});
