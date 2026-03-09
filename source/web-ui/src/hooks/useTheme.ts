import { useEffect } from "react";

/**
 * Syncs the dark/light theme with the user's OS preference.
 *
 * Adds or removes the `.dark` class on `<html>` based on
 * `prefers-color-scheme: dark`. Listens for changes so the
 * theme updates if the user toggles their OS setting.
 */
export function useTheme() {
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");

    function apply(dark: boolean) {
      document.documentElement.classList.toggle("dark", dark);
    }

    apply(mq.matches);

    function onChange(e: MediaQueryListEvent) {
      apply(e.matches);
    }

    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
}
