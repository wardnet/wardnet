import {
  type ColumnDef,
  type RowData,
  flexRender,
  getCoreRowModel,
  useReactTable,
} from "@tanstack/react-table";

import { TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/core/ui/table";
import { cn } from "@/lib/utils";

declare module "@tanstack/react-table" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface ColumnMeta<TData extends RowData, TValue> {
    className?: string;
  }
}

interface DataTableProps<TData, TValue> {
  columns: ColumnDef<TData, TValue>[];
  data: TData[];
  /** Message shown when the table has no rows. */
  emptyMessage?: string;
  /** Optional click handler per row. */
  onRowClick?: (row: TData) => void;
  /**
   * Use `table-layout: fixed` so per-column widths (declared via
   * `columnDef.meta.className`, e.g. `"w-24"`) are honored regardless
   * of cell content. Without this, the browser's default `auto`
   * layout re-fits columns to content — fine for static tables, but
   * causes visible reflow on any table whose data changes frequently
   * (live DNS log, filtered views, etc.). Opt-in so existing tables
   * that rely on auto-fit aren't disrupted.
   */
  fixedLayout?: boolean;
}

/** Reusable data table built on TanStack Table + shadcn Table primitives. */
export function DataTable<TData, TValue>({
  columns,
  data,
  emptyMessage = "No results.",
  onRowClick,
  fixedLayout = false,
}: DataTableProps<TData, TValue>) {
  const table = useReactTable({
    data,
    columns,
    getCoreRowModel: getCoreRowModel(),
  });

  return (
    // Note: no `overflow-*` here — any non-visible overflow on an ancestor
    // disables `position: sticky` on the <th> below, so headers can't pin
    // to the scroll context. We also bypass the shadcn `<Table>`
    // primitive because it wraps `<table>` in a `<div overflow-x-auto>`
    // that has the same clipping side-effect; rendering the `<table>`
    // ourselves keeps sticky positioning working all the way up to the
    // nearest scrolling ancestor (page-level main, or a per-page
    // wrapper like the DNS log's flex column).
    <div className="rounded-xl border border-line">
      <table className={cn("w-full caption-bottom text-sm", fixedLayout && "table-fixed")}>
        <TableHeader>
          {table.getHeaderGroups().map((headerGroup) => (
            <TableRow key={headerGroup.id} className="bg-sunken hover:bg-sunken">
              {headerGroup.headers.map((header) => (
                <TableHead
                  key={header.id}
                  className={cn(
                    "sticky top-0 z-10 bg-sunken px-4 py-3 text-ink-3",
                    header.column.columnDef.meta?.className,
                  )}
                >
                  {header.isPlaceholder
                    ? null
                    : flexRender(header.column.columnDef.header, header.getContext())}
                </TableHead>
              ))}
            </TableRow>
          ))}
        </TableHeader>
        <TableBody>
          {table.getRowModel().rows?.length ? (
            table.getRowModel().rows.map((row) => (
              <TableRow
                key={row.id}
                className={onRowClick ? "cursor-pointer" : undefined}
                onClick={onRowClick ? () => onRowClick(row.original) : undefined}
              >
                {row.getVisibleCells().map((cell) => (
                  <TableCell
                    key={cell.id}
                    className={cn("px-4 py-3", cell.column.columnDef.meta?.className)}
                  >
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </TableCell>
                ))}
              </TableRow>
            ))
          ) : (
            <TableRow>
              <TableCell colSpan={columns.length} className="h-24 text-center text-ink-3">
                {emptyMessage}
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </table>
    </div>
  );
}
