import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  useForwardingRules,
  useCreateForwardingRule,
  useUpdateForwardingRule,
  useDeleteForwardingRule,
} from "@wardnet/web";
import { ConditionalForwardingCard } from "@/components/features/ConditionalForwardingCard";
import { renderWithProviders } from "../../test-utils";

Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.setPointerCapture ??= () => {};
Element.prototype.releasePointerCapture ??= () => {};
Element.prototype.scrollIntoView ??= () => {};
vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    unobserve() {}
    disconnect() {}
  },
);

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useForwardingRules: vi.fn(),
    useCreateForwardingRule: vi.fn(),
    useUpdateForwardingRule: vi.fn(),
    useDeleteForwardingRule: vi.fn(),
  };
});

const createMutate = vi.fn();
const updateMutate = vi.fn();
const deleteMutate = vi.fn();

function mutation<T>(mutate: ReturnType<typeof vi.fn>): T {
  return {
    mutate,
    mutateAsync: vi.fn(),
    isPending: false,
  } as unknown as T;
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(useForwardingRules).mockReturnValue({
    data: {
      rules: [
        {
          id: "r1",
          domain: "corp.internal",
          upstream: "10.0.0.1",
          enabled: true,
        },
      ],
    },
  } as unknown as ReturnType<typeof useForwardingRules>);
  vi.mocked(useCreateForwardingRule).mockReturnValue(
    mutation<ReturnType<typeof useCreateForwardingRule>>(createMutate),
  );
  vi.mocked(useUpdateForwardingRule).mockReturnValue(
    mutation<ReturnType<typeof useUpdateForwardingRule>>(updateMutate),
  );
  vi.mocked(useDeleteForwardingRule).mockReturnValue(
    mutation<ReturnType<typeof useDeleteForwardingRule>>(deleteMutate),
  );
});

describe("ConditionalForwardingCard", () => {
  it("renders existing rules", () => {
    renderWithProviders(<ConditionalForwardingCard />);
    expect(screen.getByText("corp.internal")).toBeInTheDocument();
    expect(screen.getByText("10.0.0.1")).toBeInTheDocument();
  });

  it("toggling a rule calls updateRule.mutate", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ConditionalForwardingCard />);
    await user.click(screen.getByLabelText("Toggle rule for corp.internal"));
    expect(updateMutate).toHaveBeenCalledWith({
      id: "r1",
      body: { enabled: false },
    });
  });

  it("creates a rule through the add form", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ConditionalForwardingCard />);
    await user.click(screen.getByTestId("fwd-add"));
    await user.type(screen.getByTestId("fwd-domain"), "lab.internal");
    await user.type(screen.getByTestId("fwd-upstream"), "10.1.1.1");
    await user.click(screen.getByTestId("fwd-submit"));
    expect(createMutate).toHaveBeenCalledWith(
      { domain: "lab.internal", upstream: "10.1.1.1", enabled: true },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("cancelling closes the form", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ConditionalForwardingCard />);
    await user.click(screen.getByTestId("fwd-add"));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("fwd-domain")).not.toBeInTheDocument();
  });

  it("edits a rule via the row menu", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<ConditionalForwardingCard />);
    await user.click(screen.getByTestId("fwd-row-menu"));
    await user.click(await screen.findByTestId("fwd-edit"));
    const domain = screen.getByTestId("fwd-domain") as HTMLInputElement;
    expect(domain.value).toBe("corp.internal");
    await user.clear(domain);
    await user.type(domain, "corp.local");
    await user.click(screen.getByTestId("fwd-submit"));
    expect(updateMutate).toHaveBeenCalledWith(
      { id: "r1", body: { domain: "corp.local", upstream: "10.0.0.1" } },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("deletes a rule after confirming", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<ConditionalForwardingCard />);
    await user.click(screen.getByTestId("fwd-row-menu"));
    await user.click(await screen.findByTestId("fwd-delete"));
    await user.click(await screen.findByTestId("confirm-dialog-confirm"));
    expect(deleteMutate).toHaveBeenCalledWith("r1");
  });
});
