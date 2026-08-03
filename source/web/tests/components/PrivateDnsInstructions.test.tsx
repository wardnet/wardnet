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

  // The variant decides whether the reader is on a different screen than the
  // phone being configured (#916). It is the only thing gating the QR.
  const QR_ALT = "Private DNS configuration profile QR code";

  it("renders the QR and scan copy by default (remote reader)", async () => {
    render(
      <PrivateDnsInstructions
        hostname="tok.abc.my.wardnet.services"
        profileUrl="/api/private-dns/me/profile"
      />,
    );

    // The QR data URL is produced asynchronously, so wait for the image.
    expect(await screen.findByAltText(QR_ALT)).toBeInTheDocument();
    expect(
      screen.getByText(/Scan the QR with the iPhone camera/),
    ).toBeInTheDocument();
  });

  it("drops the QR and rewrites the iOS copy when read on the target device", async () => {
    render(
      <PrivateDnsInstructions
        hostname="tok.abc.my.wardnet.services"
        profileUrl="/api/private-dns/me/profile"
        variant="on-device"
      />,
    );

    // The phone can't scan its own screen — the profile link is the only path.
    expect(
      await screen.findByText(/Tap the link above to download the profile/),
    ).toBeInTheDocument();
    expect(screen.queryByAltText(QR_ALT)).not.toBeInTheDocument();
    expect(screen.queryByText(/Scan the QR/)).not.toBeInTheDocument();

    // Everything else survives: the link and the Android copy affordance.
    expect(screen.getByTestId("private-dns-profile-link")).toHaveAttribute(
      "href",
      `${window.location.origin}/api/private-dns/me/profile`,
    );
    expect(screen.getByTestId("private-dns-copy-hostname")).toBeInTheDocument();
  });
});
