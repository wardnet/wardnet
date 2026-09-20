import { useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import {
  Button,
  Field,
  Form,
  Input,
  Text,
  Validator,
  useRedeemEnrolment,
} from "@wardnet/web";

/**
 * Redeem a one-time enrolment invitation and set a first password.
 *
 * Unauthenticated by necessity: the person arriving here has no credential
 * yet, so there is nothing to sign in with. The token *is* the authorization
 * — single-use, expiring, and checked by the daemon in SQL.
 *
 * The token may arrive in the URL (`/redeem?token=…`), so an admin can send a
 * link rather than dictating 32 bytes of base64. It is still editable, because
 * a token passed by message or read aloud has no link to click.
 */
export default function Redeem() {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const redeem = useRedeemEnrolment();

  const [token, setToken] = useState(params.get("token") ?? "");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  function handleSubmit(values: { token: string; password: string }) {
    setFormError(null);

    // Checked here rather than by the daemon: a mistyped confirmation is a
    // typing mistake, not a rejected credential, and the API takes one
    // password because a second field is a UI concern.
    if (values.password !== confirm) {
      setFormError("Those passwords do not match.");
      return;
    }

    redeem.mutate(
      { token: values.token.trim(), password: values.password },
      {
        onSuccess: (user) => setDone(user.display_name),
        // The daemon answers the same way for an unknown, expired, or
        // already-redeemed token, so this message must not guess which.
        onError: () =>
          setFormError(
            "That invitation is not valid. It may have expired or already been used — ask your administrator for a new one.",
          ),
      },
    );
  }

  if (done) {
    return (
      <div className="login-form">
        <Text as="p">
          Your password is set, {done}. You can sign in with it now.
        </Text>
        <Button type="button" onClick={() => navigate("/login")}>
          Go to sign in
        </Button>
      </div>
    );
  }

  return (
    <Form
      values={{ token, password }}
      onSubmit={handleSubmit}
      className="login-form"
    >
      <Text as="p">
        You have been invited to this Wardnet. Choose a password — nobody else,
        including whoever invited you, will know it.
      </Text>

      <Field label="Invitation code" htmlFor="redeem-token" name="token">
        <Input
          id="redeem-token"
          data-testid="redeem-token"
          value={token}
          onChange={(e) => setToken(e.target.value)}
          autoComplete="one-time-code"
        />
      </Field>
      <Validator
        name="token"
        rule="required"
        message="An invitation code is required."
      />

      <Field label="New password" htmlFor="redeem-password" name="password">
        <Input
          id="redeem-password"
          type="password"
          data-testid="redeem-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="new-password"
        />
      </Field>
      <Validator
        name="password"
        rule="required"
        message="A password is required."
      />

      <Field label="Confirm password" htmlFor="redeem-confirm">
        <Input
          id="redeem-confirm"
          type="password"
          data-testid="redeem-confirm"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          autoComplete="new-password"
        />
      </Field>

      {formError && (
        <Text
          as="p"
          role="alert"
          data-testid="redeem-error"
          className="login-form__error"
        >
          {formError}
        </Text>
      )}

      <Button
        type="submit"
        disabled={redeem.isPending}
        data-testid="redeem-submit"
        className="login-form__submit"
      >
        {redeem.isPending ? "Setting password…" : "Set password"}
      </Button>

      <Text as="p" className="login-form__hint">
        Already have a password? <Link to="/login">Sign in</Link>.
      </Text>
    </Form>
  );
}
