import { useEffect } from "react";

/**
 * Syncs the dark/light theme with the user's OS preference.
 *
 * Sets `data-theme="dark"` / `data-theme="light"` on `<html>` so Forge's
 * token block (`[data-theme="dark"]` in design-system/styles.css) fires.
 * Listens for changes so the theme updates if the user toggles their OS
 * setting.
 */
export function useTheme() {
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");

    function apply(dark: boolean) {
      document.documentElement.dataset.theme = dark ? "dark" : "light";
    }

    apply(mq.matches);

    function onChange(e: MediaQueryListEvent) {
      apply(e.matches);
    }

    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
}
