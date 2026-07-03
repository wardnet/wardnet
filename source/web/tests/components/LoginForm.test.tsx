import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WardnetApiError } from "@wardnet/js";
import { LoginForm } from "../../src/components/LoginForm";

async function fillCredentials(
  user: ReturnType<typeof userEvent.setup>,
  username = "admin",
  password = "secret",
) {
  await user.type(screen.getByTestId("login-username"), username);
  await user.type(screen.getByTestId("login-password"), password);
}

describe("LoginForm", () => {
  beforeEach(() => vi.clearAllMocks());

  it("logs in and reports success with the username", async () => {
    const user = userEvent.setup();
    const login = vi.fn().mockResolvedValue(undefined);
    const onSuccess = vi.fn();
    render(<LoginForm login={login} onSuccess={onSuccess} />);
    await fillCredentials(user);
    await user.click(screen.getByTestId("login-submit"));
    await waitFor(() =>
      expect(login).toHaveBeenCalledWith("admin", "secret", false),
    );
    expect(onSuccess).toHaveBeenCalledWith("admin");
  });

  it("shows a credential error on a 401", async () => {
    const user = userEvent.setup();
    const login = vi
      .fn()
      .mockRejectedValue(
        new WardnetApiError(401, "Unauthorized", { error: "unauthorized" }),
      );
    render(<LoginForm login={login} />);
    await fillCredentials(user);
    await user.click(screen.getByTestId("login-submit"));
    expect(await screen.findByTestId("login-error")).toHaveTextContent(
      "Invalid username or password.",
    );
  });

  it("shows a connection error on a non-auth failure", async () => {
    const user = userEvent.setup();
    const login = vi.fn().mockRejectedValue(new Error("network"));
    render(<LoginForm login={login} />);
    await fillCredentials(user);
    await user.click(screen.getByTestId("login-submit"));
    expect(await screen.findByTestId("login-error")).toHaveTextContent(
      "Unable to connect to daemon. Is it running?",
    );
  });

  it("passes rememberMe=true through the checkbox variant", async () => {
    const user = userEvent.setup();
    const login = vi.fn().mockResolvedValue(undefined);
    render(<LoginForm login={login} rememberMe="checkbox" />);
    await fillCredentials(user);
    await user.click(screen.getByRole("checkbox"));
    await user.click(screen.getByTestId("login-submit"));
    await waitFor(() =>
      expect(login).toHaveBeenCalledWith("admin", "secret", true),
    );
  });

  it("does not call login when required fields are empty", async () => {
    const user = userEvent.setup();
    const login = vi.fn();
    render(<LoginForm login={login} />);
    await user.click(screen.getByTestId("login-submit"));
    expect(login).not.toHaveBeenCalled();
  });
});
