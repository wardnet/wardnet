import { Toaster as Sonner, type ToasterProps } from "sonner";
import {
  CircleCheckIcon,
  InfoIcon,
  TriangleAlertIcon,
  OctagonXIcon,
  Loader2Icon,
} from "lucide-react";

/**
 * Forge §14 — toast surface for auto-save / validation feedback.
 *
 * Sonner emits `[data-sonner-toaster]` + `[data-sonner-toast][data-type]`
 * at runtime; the visual contract (card surface, --line border tinted by
 * tone, --shadow-pop, --radius corners) lives in `forge/styles.css` next
 * to the `.toast` mock so CSS-only consumers and the Sonner runtime
 * land on the same look. We forward `.toast` / `.toast--ok` / etc. via
 * `toastOptions.classNames` so the same selectors fire whether a
 * stylesheet author targets the Forge class or Sonner's data attribute.
 *
 * Theme is left as `system`; the @theme bridge in `index.css` and Forge's
 * own `[data-theme="dark"]` block flip card / line / ink tokens together,
 * so per-toast theming is unnecessary.
 */
const Toaster = ({ ...props }: ToasterProps) => {
  return (
    <Sonner
      theme="system"
      className="toaster group"
      icons={{
        success: <CircleCheckIcon className="size-4" />,
        info: <InfoIcon className="size-4" />,
        warning: <TriangleAlertIcon className="size-4" />,
        error: <OctagonXIcon className="size-4" />,
        loading: <Loader2Icon className="size-4 animate-spin" />,
      }}
      toastOptions={{
        classNames: {
          toast: "toast",
          success: "toast--ok",
          info: "toast--info",
          warning: "toast--warn",
          error: "toast--down",
        },
      }}
      {...props}
    />
  );
};

export { Toaster };
