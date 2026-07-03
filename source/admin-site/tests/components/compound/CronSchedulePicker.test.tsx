import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CronSchedulePicker } from "@/components/compound/CronSchedulePicker";
import { renderWithProviders } from "../../test-utils";

// Radix Popover/Select need these DOM APIs jsdom doesn't provide.
beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
  Element.prototype.hasPointerCapture = vi.fn();
  Element.prototype.releasePointerCapture = vi.fn();
  Element.prototype.setPointerCapture = vi.fn();
});

async function openPopover(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button"));
}

describe("CronSchedulePicker", () => {
  it("renders a trigger button with a human-readable label", () => {
    renderWithProviders(
      <CronSchedulePicker
        value="0 3 * * *"
        onChange={vi.fn()}
        label="Schedule"
      />,
    );
    expect(screen.getByRole("button")).toBeInTheDocument();
  });

  it("opens the popover and shows only the repeat field for hourly", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <CronSchedulePicker value="0 * * * *" onChange={vi.fn()} />,
    );
    await openPopover(user);
    expect(await screen.findByText("Repeat")).toBeInTheDocument();
    expect(screen.queryByText("Interval")).not.toBeInTheDocument();
    expect(screen.queryByText("At")).not.toBeInTheDocument();
  });

  it("shows the interval field for every-N-hours schedules", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <CronSchedulePicker value="0 */3 * * *" onChange={vi.fn()} />,
    );
    await openPopover(user);
    expect(await screen.findByText("Interval")).toBeInTheDocument();
  });

  it("shows the time field for daily schedules", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <CronSchedulePicker value="0 5 * * *" onChange={vi.fn()} />,
    );
    await openPopover(user);
    expect(await screen.findByText("At")).toBeInTheDocument();
  });

  it("shows weekday buttons for weekly schedules", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <CronSchedulePicker value="0 3 * * 1,3" onChange={vi.fn()} />,
    );
    await openPopover(user);
    expect(await screen.findByText("Days")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Wednesday" }),
    ).toBeInTheDocument();
  });

  it("shows the day-of-month field for monthly schedules", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <CronSchedulePicker value="0 3 15 * *" onChange={vi.fn()} />,
    );
    await openPopover(user);
    expect(await screen.findByText("Day of month")).toBeInTheDocument();
  });

  it("falls back to daily for an unparseable expression", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <CronSchedulePicker value="not a cron" onChange={vi.fn()} />,
    );
    await openPopover(user);
    expect(await screen.findByText("At")).toBeInTheDocument();
  });

  it("falls back to daily when the weekday list is invalid", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <CronSchedulePicker value="0 3 * * 9" onChange={vi.fn()} />,
    );
    await openPopover(user);
    expect(await screen.findByText("At")).toBeInTheDocument();
  });

  it("adds a weekday and emits a new cron string", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithProviders(
      <CronSchedulePicker value="0 3 * * 1,3" onChange={onChange} />,
    );
    await openPopover(user);
    await user.click(await screen.findByRole("button", { name: "Friday" }));
    expect(onChange).toHaveBeenCalledWith("0 3 * * 1,3,5");
  });

  it("refuses to clear the last selected weekday", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithProviders(
      <CronSchedulePicker value="0 3 * * 2" onChange={onChange} />,
    );
    await openPopover(user);
    await user.click(await screen.findByRole("button", { name: "Tuesday" }));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("closes the popover when Done is clicked", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <CronSchedulePicker value="0 3 * * *" onChange={vi.fn()} />,
    );
    await openPopover(user);
    const done = await screen.findByRole("button", { name: "Done" });
    await user.click(done);
    expect(screen.queryByText("Repeat")).not.toBeInTheDocument();
  });
});
