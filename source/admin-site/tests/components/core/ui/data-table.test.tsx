import type { ColumnDef } from "@tanstack/react-table";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { DataTable, RowAction } from "@/components/core/ui/data-table";

interface Row {
  id: string;
  name: string;
}

const columns: ColumnDef<Row>[] = [
  { accessorKey: "name", header: "Name", meta: { className: "w-24" } },
];

const data: Row[] = [
  { id: "1", name: "Alpha" },
  { id: "2", name: "Beta" },
];

describe("DataTable", () => {
  it("renders the empty message when there are no rows", () => {
    render(
      <DataTable columns={columns} data={[]} emptyMessage="Nothing here" />,
    );
    expect(screen.getByText("Nothing here")).toBeInTheDocument();
  });

  it("renders rows and fires onRowClick", async () => {
    const user = userEvent.setup();
    const onRowClick = vi.fn();
    render(
      <DataTable
        columns={columns}
        data={data}
        fixedLayout
        onRowClick={onRowClick}
      />,
    );
    expect(screen.getByText("Alpha")).toBeInTheDocument();
    await user.click(screen.getByText("Beta"));
    expect(onRowClick).toHaveBeenCalledWith(data[1]);
  });

  it("renders the toolbar with groups, search, filters, add CTA and action", async () => {
    const user = userEvent.setup();
    const onSearchChange = vi.fn();
    const onAdd = vi.fn();
    const onGroupChange = vi.fn();
    render(
      <DataTable
        columns={columns}
        data={data}
        groups={[
          { id: "all", label: "All", count: 2 },
          { id: "some", label: "Some" },
        ]}
        activeGroup="all"
        onGroupChange={onGroupChange}
        searchValue="x"
        onSearchChange={onSearchChange}
        searchTestId="tbl-search"
        filters={<div>my-filter</div>}
        addLabel="Add row"
        onAdd={onAdd}
        addTestId="tbl-add"
        action={<span>extra-action</span>}
      />,
    );
    expect(screen.getByText("my-filter")).toBeInTheDocument();
    expect(screen.getByText("extra-action")).toBeInTheDocument();

    await user.type(screen.getByTestId("tbl-search"), "y");
    expect(onSearchChange).toHaveBeenCalled();

    await user.click(screen.getByTestId("tbl-add"));
    expect(onAdd).toHaveBeenCalled();
  });

  it("renders per-row overflow actions", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <DataTable
        columns={columns}
        data={data}
        rowActionsTestId="row-actions"
        rowActions={() => (
          <RowAction onSelect={onSelect} destructive testId="delete-action">
            Delete
          </RowAction>
        )}
      />,
    );
    const triggers = screen.getAllByTestId("row-actions");
    await user.click(triggers[0]);
    await user.click(await screen.findByTestId("delete-action"));
    expect(onSelect).toHaveBeenCalled();
  });
});
