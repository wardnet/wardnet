import { type ReactNode } from "react";
import {
  type ColumnDef,
  type RowData,
  flexRender,
  getCoreRowModel,
  useReactTable,
} from "@tanstack/react-table";
import { MoreHorizontal, Plus, Search } from "lucide-react";

import { Button } from "@wardnet/web";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import { SegmentedTabs } from "@wardnet/web";
import { cn } from "@wardnet/web";

declare module "@tanstack/react-table" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface ColumnMeta<TData extends RowData, TValue> {
    className?: string;
  }
}

export interface DataTableGroup {
  id: string;
  label: string;
  count?: number;
  /** Optional `data-testid` for this group's tab, forwarded from the
   *  consuming component so the generic primitive carries no baked-in
   *  test contract (e2e selector ADR). */
  testId?: string;
}

export interface RowActionProps {
  /** Fires when the user clicks (or keyboard-activates) the row action. */
  onSelect?: () => void;
  /** Renders the item in the danger tone (red text) so destructive
   *  actions like "Delete" are visually distinguishable from
   *  routine ones inside the same overflow menu. */
  destructive?: boolean;
  /** Optional leading icon (rendered before the label). */
  icon?: ReactNode;
  /** Optional `data-testid` for the menu item so e2e specs can target
   *  a specific action without relying on its label text. */
  testId?: string;
  children: ReactNode;
}

/** Single entry inside a row's overflow (`…`) menu.
 *
 *  Pass one or more of these from `DataTable`'s `rowActions` render
 *  prop. The DataTable handles the trigger button and the dropdown
 *  container so consumers only describe what each action does. */
export function RowAction({
  onSelect,
  destructive,
  icon,
  testId,
  children,
}: RowActionProps) {
  return (
    <DropdownMenuItem
      onSelect={onSelect}
      variant={destructive ? "destructive" : "default"}
      data-testid={testId}
    >
      {icon}
      <span>{children}</span>
    </DropdownMenuItem>
  );
}

interface DataTableProps<TData, TValue> {
  columns: ColumnDef<TData, TValue>[];
  data: TData[];
  /**
   * Shown when the table has no rows. Usually a plain string; accepts a node
   * so a table can explain *why* it is empty and offer a way forward — see the
   * Devices page, where a missed MAC search suggests near-miss candidates
   * (issue #1099) — without bolting a separate panel below the grid.
   */
  emptyMessage?: ReactNode;
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
  /**
   * Optional segmented group selector rendered on the left of the
   * toolbar. Provide `groups` plus `activeGroup` / `onGroupChange` to
   * render a `.tabs` pill control with optional count badges.
   */
  groups?: DataTableGroup[];
  activeGroup?: string;
  onGroupChange?: (id: string) => void;
  /** Controlled search input rendered after the group control. */
  searchValue?: string;
  onSearchChange?: (value: string) => void;
  searchPlaceholder?: string;
  /** Optional filter controls (e.g. Select dropdowns) rendered
   *  between the group control and the search input. */
  filters?: ReactNode;
  /** Standard "add new entry" CTA — when both are set, the DataTable
   *  renders an outline button on the far right of the toolbar. This
   *  is the preferred way to attach an Add action to a table; it
   *  encapsulates the agreed variant + height so consumers don't
   *  reimplement it. */
  addLabel?: string;
  onAdd?: () => void;
  /** Optional `data-testid`s forwarded onto the toolbar search input,
   *  the Add CTA, and each row's overflow-menu trigger. Kept as props
   *  (not literals) so this shared primitive holds no consumer-specific
   *  test contract — the consuming compound/feature component owns it. */
  searchTestId?: string;
  addTestId?: string;
  rowActionsTestId?: string;
  /** Free-form action slot — escape hatch for non-standard toolbar
   *  controls (multiple buttons, a custom widget). Prefer `addLabel`
   *  + `onAdd` for the common "Add X" case. */
  action?: ReactNode;
  /** Per-row overflow menu. Return one or more `<RowAction>` elements
   *  for each row; the DataTable renders a `…` trigger on the right
   *  edge of every row and opens a dropdown menu when activated.
   *  The trigger swallows the row-click handler so opening the menu
   *  doesn't trigger navigation. */
  rowActions?: (row: TData) => ReactNode;
}

/**
 * Forge §05 data-table primitive. The `.tbl` Forge class owns the
 * visual contract; this wrapper renders the TanStack header/body
 * groups onto native <thead>/<tbody>. An optional `.tbl-toolbar` row
 * is rendered above the table when any of `groups` / `onSearchChange`
 * / `action` is provided.
 *
 * The outer `.tbl-wrap` mirrors `.card.card--flush` (border + shadow
 * on --bg-card) but deliberately omits `overflow: hidden`: any
 * non-visible overflow on an ancestor turns that ancestor into the
 * scroll container for sticky positioning, which would prevent the
 * <th> from pinning to the page-level scroll.
 */
export function DataTable<TData, TValue>({
  columns,
  data,
  emptyMessage = "No results.",
  onRowClick,
  fixedLayout = false,
  groups,
  activeGroup,
  onGroupChange,
  searchValue,
  onSearchChange,
  searchPlaceholder = "Search…",
  filters,
  addLabel,
  onAdd,
  searchTestId,
  addTestId,
  rowActionsTestId,
  action,
  rowActions,
}: DataTableProps<TData, TValue>) {
  // eslint-disable-next-line react-hooks/incompatible-library
  const table = useReactTable({
    data,
    columns,
    getCoreRowModel: getCoreRowModel(),
  });

  const rows = table.getRowModel().rows;
  const hasAddCta = !!addLabel && !!onAdd;
  const showToolbar =
    !!groups ||
    onSearchChange !== undefined ||
    !!filters ||
    hasAddCta ||
    !!action;

  return (
    <div className="tbl-frame">
      {showToolbar && (
        <div className="tbl-toolbar">
          {groups && groups.length > 0 && (
            <>
              {/* Desktop wrapper — shared segmented pill control. Wrapped
                  in a plain div because Forge's unlayered `.tabs {
                  display: inline-flex }` would otherwise beat Tailwind's
                  layered `.hidden { display: none }`. `DataTableGroup` is
                  structurally a `SegmentedTab` (id/label/count/testId), so
                  the groups pass straight through. */}
              <div className="hidden md:block">
                <SegmentedTabs
                  tabs={groups}
                  activeId={activeGroup ?? ""}
                  onChange={(id) => onGroupChange?.(id)}
                />
              </div>
              {/* Mobile wrapper — same data as a Select that collapses
                  long group lists to a single 34px trigger. Same
                  wrapping trick as above to win against Forge's
                  unlayered `.select-trigger` display rule. */}
              <div className="md:hidden">
                <Select
                  value={activeGroup}
                  onValueChange={(v) => onGroupChange?.(v)}
                >
                  <SelectTrigger className="select-trigger--md w-40">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {groups.map((g) => (
                      <SelectItem key={g.id} value={g.id}>
                        {g.label}
                        {g.count !== undefined ? ` · ${g.count}` : ""}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </>
          )}
          {filters}
          {onSearchChange && (
            <label className="tbl-toolbar__search">
              <Search aria-hidden="true" />
              <input
                type="search"
                value={searchValue ?? ""}
                onChange={(e) => onSearchChange(e.target.value)}
                placeholder={searchPlaceholder}
                data-testid={searchTestId}
              />
            </label>
          )}
          {hasAddCta && (
            <Button variant="outline" onClick={onAdd} data-testid={addTestId}>
              <Plus aria-hidden />
              {addLabel}
            </Button>
          )}
          {action}
        </div>
      )}
      <div className="tbl-wrap">
        <table className={cn("tbl", fixedLayout && "tbl--fixed")}>
          <thead>
            {table.getHeaderGroups().map((headerGroup) => (
              <tr key={headerGroup.id}>
                {headerGroup.headers.map((header) => (
                  <th
                    key={header.id}
                    className={header.column.columnDef.meta?.className}
                  >
                    {header.isPlaceholder
                      ? null
                      : flexRender(
                          header.column.columnDef.header,
                          header.getContext(),
                        )}
                  </th>
                ))}
                {rowActions && (
                  <th
                    className="tbl__row-actions-head"
                    aria-label="Row actions"
                  />
                )}
              </tr>
            ))}
          </thead>
          <tbody>
            {rows.length ? (
              rows.map((row) => {
                // Resolve this row's actions once. A row can legitimately have
                // none (e.g. an expired DHCP lease); in that case we keep the
                // cell for column alignment but omit the `…` trigger so it
                // can't open an empty dropdown.
                const actions = rowActions?.(row.original);
                const menu = actions ? (
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label="Row actions"
                        data-testid={rowActionsTestId}
                      >
                        <MoreHorizontal />
                      </Button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent>{actions}</DropdownMenuContent>
                  </DropdownMenu>
                ) : null;
                return (
                  <tr
                    key={row.id}
                    data-clickable={onRowClick ? "true" : undefined}
                    onClick={
                      onRowClick ? () => onRowClick(row.original) : undefined
                    }
                  >
                    {row.getVisibleCells().map((cell) => (
                      <td
                        key={cell.id}
                        className={cell.column.columnDef.meta?.className}
                      >
                        {flexRender(
                          cell.column.columnDef.cell,
                          cell.getContext(),
                        )}
                      </td>
                    ))}
                    {rowActions && (
                      <td
                        className="tbl__row-actions-cell"
                        onClick={(e) => e.stopPropagation()}
                      >
                        {menu}
                      </td>
                    )}
                  </tr>
                );
              })
            ) : (
              <tr>
                <td
                  colSpan={columns.length + (rowActions ? 1 : 0)}
                  className="tbl-empty"
                >
                  {emptyMessage}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
