import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableFooter,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/core/ui/table";

describe("table primitives", () => {
  it("renders all slots with forwarded className", () => {
    render(
      <Table className="my-table">
        <TableCaption className="cap">Caption</TableCaption>
        <TableHeader className="hd">
          <TableRow>
            <TableHead className="th">Head</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow>
            <TableCell className="cell">Body</TableCell>
          </TableRow>
        </TableBody>
        <TableFooter className="ft">
          <TableRow>
            <TableCell>Foot</TableCell>
          </TableRow>
        </TableFooter>
      </Table>,
    );

    expect(screen.getByText("Caption")).toBeInTheDocument();
    expect(screen.getByText("Head")).toBeInTheDocument();
    expect(screen.getByText("Body")).toBeInTheDocument();
    expect(screen.getByText("Foot")).toBeInTheDocument();
    expect(document.querySelector('[data-slot="table"]')).toHaveClass(
      "my-table",
    );
    expect(
      document.querySelector('[data-slot="table-container"]'),
    ).toBeInTheDocument();
  });
});
