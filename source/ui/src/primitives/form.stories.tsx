import * as React from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { Form, Validator } from "./form";
import { Field } from "./field";
import { Input } from "./input";
import { Button } from "./button";

const meta = {
  title: "Primitives/Form",
  component: Form,
  tags: ["autodocs"],
  args: { values: {}, onSubmit: () => {}, children: null },
} satisfies Meta<typeof Form>;

export default meta;
type Story = StoryObj<typeof meta>;

/**
 * The Form coordinates validation via context: `<Validator>`s register rules
 * keyed by field `name`, errors surface on the matching `<Field name="…">`,
 * and `onSubmit` fires only when every rule passes.
 */
export const WithValidation: Story = {
  render: () => {
    const [values, setValues] = React.useState({ hostname: "", port: "" });
    const [submitted, setSubmitted] = React.useState<string | null>(null);
    return (
      <div style={{ maxWidth: 360 }}>
        <Form
          values={values}
          onSubmit={(v) => setSubmitted(JSON.stringify(v))}
          className="flex flex-col gap-3"
        >
          <Validator name="hostname" rule="required" />
          <Validator
            name="port"
            validate={(v) =>
              /^\d+$/.test(String(v)) ? null : "Port must be numeric."
            }
          />
          <Field label="Hostname" htmlFor="hostname" name="hostname">
            <Input
              id="hostname"
              value={values.hostname}
              onChange={(e) =>
                setValues((s) => ({ ...s, hostname: e.target.value }))
              }
            />
          </Field>
          <Field label="Port" htmlFor="port" name="port">
            <Input
              id="port"
              value={values.port}
              onChange={(e) =>
                setValues((s) => ({ ...s, port: e.target.value }))
              }
            />
          </Field>
          <div className="flex justify-end">
            <Button type="submit">Save</Button>
          </div>
        </Form>
        {submitted && <p style={{ marginTop: 12 }}>Submitted: {submitted}</p>}
      </div>
    );
  },
};
