import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { CreateReservationInline } from "@/components/features/CreateReservationInline";
import { makeDevice, renderWithProviders } from "../../test-utils";

const HELP = "Suggested from device name";

function hostnameInput(): HTMLInputElement {
  return screen.getByTestId("dhcp-reservation-hostname") as HTMLInputElement;
}

/** Fill the segmented MAC input (6 hex inputs) from a plain MAC string. */
async function typeMac(user: ReturnType<typeof userEvent.setup>, mac: string) {
  const segments = mac.split(":");
  const inputs = within(
    screen.getByTestId("dhcp-reservation-mac"),
  ).getAllByRole("textbox");
  for (let i = 0; i < segments.length; i++) {
    // eslint-disable-next-line security/detect-object-injection -- loop index over locally queried MAC segment inputs in a test helper
    await user.click(inputs[i]);
    // eslint-disable-next-line security/detect-object-injection -- loop index over segments split from a test-local literal MAC string
    await user.keyboard(segments[i]);
  }
}

describe("CreateReservationInline hostname suggestion (issue #85)", () => {
  it("pre-fills and marks the hostname when the fixed MAC matches a device", () => {
    renderWithProviders(
      <CreateReservationInline
        onClose={vi.fn()}
        defaults={{ mac: "AA:BB:CC:DD:EE:FF" }}
        devices={[
          makeDevice({ mac: "AA:BB:CC:DD:EE:FF", name: "Office printer" }),
        ]}
      />,
    );

    expect(hostnameInput().value).toBe("Office printer");
    expect(screen.getByText(HELP)).toBeInTheDocument();
  });

  it("respects an explicit default hostname and shows no suggestion", () => {
    renderWithProviders(
      <CreateReservationInline
        onClose={vi.fn()}
        defaults={{ mac: "AA:BB:CC:DD:EE:FF", hostname: "from-lease" }}
        devices={[
          makeDevice({ mac: "AA:BB:CC:DD:EE:FF", name: "Office printer" }),
        ]}
      />,
    );

    expect(hostnameInput().value).toBe("from-lease");
    expect(screen.queryByText(HELP)).not.toBeInTheDocument();
  });

  it("leaves the hostname blank when the MAC matches no device", () => {
    renderWithProviders(
      <CreateReservationInline
        onClose={vi.fn()}
        defaults={{ mac: "11:22:33:44:55:66" }}
        devices={[
          makeDevice({ mac: "AA:BB:CC:DD:EE:FF", name: "Office printer" }),
        ]}
      />,
    );

    expect(hostnameInput().value).toBe("");
    expect(screen.queryByText(HELP)).not.toBeInTheDocument();
  });

  it("suggests as the user types a matching MAC", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <CreateReservationInline
        onClose={vi.fn()}
        devices={[
          makeDevice({ mac: "AA:BB:CC:DD:EE:FF", name: "Living-room TV" }),
        ]}
      />,
    );

    expect(hostnameInput().value).toBe("");
    await typeMac(user, "AA:BB:CC:DD:EE:FF");

    expect(hostnameInput().value).toBe("Living-room TV");
    expect(screen.getByText(HELP)).toBeInTheDocument();
  });

  it("never overwrites a hostname the user typed themselves", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <CreateReservationInline
        onClose={vi.fn()}
        devices={[
          makeDevice({ mac: "AA:BB:CC:DD:EE:FF", name: "Living-room TV" }),
        ]}
      />,
    );

    await user.type(hostnameInput(), "my-own-name");
    await typeMac(user, "AA:BB:CC:DD:EE:FF");

    expect(hostnameInput().value).toBe("my-own-name");
    expect(screen.queryByText(HELP)).not.toBeInTheDocument();
  });
});
