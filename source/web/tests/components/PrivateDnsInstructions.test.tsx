import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { PrivateDnsInstructions } from "../../src/components/PrivateDnsInstructions";

describe("PrivateDnsInstructions", () => {
  it("shows the hostname, a copy affordance, and both platform blocks", () => {
    render(
      <PrivateDnsInstructions
        hostname="tok.abc.my.wardnet.services"
        profileUrl="/api/private-dns/me/profile"
      />,
    );

    expect(screen.getByTestId("private-dns-hostname")).toHaveTextContent(
      "tok.abc.my.wardnet.services",
    );
    expect(screen.getByTestId("private-dns-copy-hostname")).toBeInTheDocument();
    // Both platform blocks are always shown (the issue wants per-platform
    // instructions). Target the section headings specifically.
    expect(
      screen.getByRole("heading", { name: "Android" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /iPhone/ })).toBeInTheDocument();
  });

  it("resolves an app-relative profile URL to an absolute link", () => {
    render(
      <PrivateDnsInstructions
        hostname="tok.abc.my.wardnet.services"
        profileUrl="/api/private-dns/me/profile"
      />,
    );

    const link = screen.getByTestId("private-dns-profile-link");
    // The QR/link must be absolute (resolved against the current origin) so a
    // phone scanning on-LAN can reach the box.
    expect(link).toHaveAttribute(
      "href",
      `${window.location.origin}/api/private-dns/me/profile`,
    );
  });

  it("falls back to the raw URL when the profile URL can't be parsed", () => {
    // An unparseable URL degrades to the input rather than throwing.
    render(
      <PrivateDnsInstructions
        hostname="tok.abc.my.wardnet.services"
        profileUrl="http://["
      />,
    );
    expect(screen.getByTestId("private-dns-profile-link")).toHaveAttribute(
      "href",
      "http://[",
    );
  });
});
