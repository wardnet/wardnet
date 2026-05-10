import {
  type ColumnDef,
  type RowData,
  flexRender,
  getCoreRowModel,
  useReactTable,
} from "@tanstack/react-table";

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

/**
 * Forge §05 data-table primitive. The `.tbl` Forge class owns the
 * visual contract — uppercase header on --bg-elev, hairline row
 * dividers, hover tint, sticky <th> — so this wrapper just attaches
 * the class and renders the TanStack header/body groups onto native
 * <thead>/<tbody>. `.host` row markup is contributed by callers via
 * the column `cell` renderer (see HostCell in compound/) so this
 * primitive stays unaware of row content.
 *
 * The outer `.tbl-wrap` mirrors `.card.card--flush` (border + shadow
 * on --bg-card) but deliberately omits `overflow: hidden`: any
 * non-visible overflow on an ancestor turns that ancestor into the
 * scroll container for sticky positioning, which would prevent the
 * <th> from pinning to the page-level scroll. We render the <table>
 * directly under the wrapper for the same reason — no inner div
 * with overflow.
 */
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

  const rows = table.getRowModel().rows;

  return (
    <div className="tbl-wrap">
      <table className={cn("tbl", fixedLayout && "tbl--fixed")}>
        <thead>
          {table.getHeaderGroups().map((headerGroup) => (
            <tr key={headerGroup.id}>
              {headerGroup.headers.map((header) => (
                <th key={header.id} className={header.column.columnDef.meta?.className}>
                  {header.isPlaceholder
                    ? null
                    : flexRender(header.column.columnDef.header, header.getContext())}
                </th>
              ))}
            </tr>
          ))}
        </thead>
        <tbody>
          {rows.length ? (
            rows.map((row) => (
              <tr
                key={row.id}
                data-clickable={onRowClick ? "true" : undefined}
                onClick={onRowClick ? () => onRowClick(row.original) : undefined}
              >
                {row.getVisibleCells().map((cell) => (
                  <td key={cell.id} className={cell.column.columnDef.meta?.className}>
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </td>
                ))}
              </tr>
            ))
          ) : (
            <tr>
              <td colSpan={columns.length} className="tbl-empty">
                {emptyMessage}
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
