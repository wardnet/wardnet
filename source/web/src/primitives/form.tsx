import * as React from "react";

/** Validator function — return an error message string when the value
 *  is invalid, or `null` / `undefined` when it passes. */
export type Validate = (value: unknown) => string | null | undefined;

function errorsEqual(
  a: Record<string, string[]>,
  b: Record<string, string[]>,
): boolean {
  const ak = Object.keys(a);
  const bk = Object.keys(b);
  if (ak.length !== bk.length) return false;
  for (const k of ak) {
    const av = a[k];
    const bv = b[k];
    if (!bv || av.length !== bv.length) return false;
    for (let i = 0; i < av.length; i++) {
      if (av[i] !== bv[i]) return false;
    }
  }
  return true;
}

interface FormContextValue {
  values: Record<string, unknown>;
  errors: Record<string, string[]>;
  registerValidator: (name: string, validate: Validate) => () => void;
}

const FormContext = React.createContext<FormContextValue | null>(null);

export function useFormContext(): FormContextValue | null {
  return React.useContext(FormContext);
}

interface FormProps<T extends Record<string, unknown>> extends Omit<
  React.ComponentProps<"form">,
  "onSubmit"
> {
  /** Controlled field values keyed by the same `name` consumers pass to
   *  `<Validator>`. The Form does not own field state; the consumer keeps
   *  its useState and passes the snapshot in. */
  values: T;
  /** Called only when every registered validator passes on submit. */
  onSubmit: (values: T) => void | Promise<void>;
  children: React.ReactNode;
}

/**
 * Validation-aware `<form>` container. Coordinates child `<Validator>`s
 * via context: on submit every registered rule runs against `values`,
 * all errors are collected (not just the first), and onSubmit only
 * fires when there are zero errors.
 *
 * `<Field>` reads its errors from this context via its `name` prop, so
 * the inline `data-invalid` styling and `.field-error` line render
 * automatically without consumers wiring per-field error props.
 */
function Form<T extends Record<string, unknown>>({
  values,
  onSubmit,
  children,
  className,
  ...rest
}: FormProps<T>) {
  const [errors, setErrors] = React.useState<Record<string, string[]>>({});
  // Track whether the user has tried to submit at least once. Before
  // that we stay silent — errors should not appear on a pristine form.
  // After the first submit, we re-run validators on every change so
  // the user gets prompt feedback as they correct the values.
  const [submitted, setSubmitted] = React.useState(false);
  // Validators are keyed by field name; multiple rules per field are
  // supported (one Validator per rule). We use a Set keyed by the
  // function identity so a Validator's deregister callback on unmount
  // removes the exact entry it added.
  const validatorsRef = React.useRef<Map<string, Set<Validate>>>(new Map());

  const registerValidator = React.useCallback(
    (name: string, validate: Validate) => {
      let bucket = validatorsRef.current.get(name);
      if (!bucket) {
        bucket = new Set();
        validatorsRef.current.set(name, bucket);
      }
      bucket.add(validate);
      return () => {
        validatorsRef.current.get(name)?.delete(validate);
      };
    },
    [],
  );

  const computeErrors = React.useCallback(
    (snapshot: T): Record<string, string[]> => {
      const next: Record<string, string[]> = {};
      for (const [name, set] of validatorsRef.current) {
        const msgs: string[] = [];
        for (const v of set) {
          const r = v(snapshot[name]);
          if (r) msgs.push(r);
        }
        if (msgs.length) next[name] = msgs;
      }
      return next;
    },
    [],
  );

  // Keep the latest `values` on a ref so the debounced timer always
  // reads the most recent snapshot, independent of the closure that
  // scheduled it.
  const valuesRef = React.useRef(values);
  valuesRef.current = values;

  // A primitive signature of the current values content. JS interns
  // primitive strings so equal-content keys compare `Object.is`-true,
  // which means we can put this in an effect dep array and the
  // effect only fires when content actually changes — not on every
  // render where the consumer hands us a new inline `values` object
  // reference. Using a key (not the `values` object itself) is what
  // breaks the "I have to submit again to clear the error" loop.
  const valuesKey = React.useMemo(() => {
    return Object.entries(values)
      .map(([k, v]) => {
        if (v instanceof File)
          return `${k}:${v.name}:${v.size}:${v.lastModified}`;
        if (v === null || v === undefined) return `${k}:`;
        if (typeof v === "object") {
          try {
            return `${k}:${JSON.stringify(v)}`;
          } catch {
            return `${k}:?`;
          }
        }
        return `${k}:${String(v)}`;
      })
      .join("|");
  }, [values]);

  // Debounced live re-validation. Once the user has submitted, any
  // edit to a field reruns the validators ~300ms later so errors
  // clear (or update) without an explicit re-submit. The effect is
  // keyed on `valuesKey` rather than `values` so it only re-runs on
  // real content changes, and the inner setErrors bails out via
  // `errorsEqual` when the recomputed map matches the current one.
  React.useEffect(() => {
    if (!submitted) return;
    const handle = window.setTimeout(() => {
      const next = computeErrors(valuesRef.current);
      setErrors((prev) => (errorsEqual(prev, next) ? prev : next));
    }, 300);
    return () => window.clearTimeout(handle);
  }, [valuesKey, submitted, computeErrors]);

  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    setSubmitted(true);
    const next = computeErrors(values);
    setErrors(next);
    if (Object.keys(next).length === 0) {
      void onSubmit(values);
    }
  };

  const ctx = React.useMemo<FormContextValue>(
    () => ({
      values: values as Record<string, unknown>,
      errors,
      registerValidator,
    }),
    [values, errors, registerValidator],
  );

  return (
    <FormContext.Provider value={ctx}>
      <form {...rest} className={className} onSubmit={handleSubmit} noValidate>
        {children}
      </form>
    </FormContext.Provider>
  );
}

interface ValidatorProps {
  /** Field name — must match the `name` prop of the corresponding `<Field>`
   *  and the key of `values` on the parent `<Form>`. */
  name: string;
  /** Built-in rule shorthand. Today: `"required"`. Extend here when new
   *  OOB rules are needed (e.g. `"email"`, `"min-length"`). */
  rule?: "required";
  /** Override the default message for the built-in rule. */
  message?: string;
  /** Custom validator function — receives the field's current value and
   *  returns an error string or `null`/`undefined`. Takes precedence
   *  over `rule` if both are provided. */
  validate?: Validate;
}

/** Declarative validation rule. Renders nothing — it only registers a
 *  validator with the parent `<Form>`. Compose multiple `<Validator>`s
 *  on the same field to enforce multiple rules; all failing messages
 *  surface on submit. */
function Validator({ name, rule, message, validate }: ValidatorProps) {
  const ctx = useFormContext();

  const fn = React.useMemo<Validate | null>(() => {
    if (validate) return validate;
    if (rule === "required") {
      const msg = message ?? "This field is required.";
      return (v) => {
        if (v === null || v === undefined) return msg;
        if (typeof v === "string" && v.trim() === "") return msg;
        if (Array.isArray(v) && v.length === 0) return msg;
        return null;
      };
    }
    return null;
  }, [rule, message, validate]);

  React.useEffect(() => {
    if (!ctx || !fn) return;
    return ctx.registerValidator(name, fn);
  }, [ctx, name, fn]);

  return null;
}

export { Form, Validator, FormContext };
