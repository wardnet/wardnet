import { useEffect, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { Hero } from "@/components/layouts/Hero";
import { Navbar } from "@/components/layouts/Navbar";
import { Features } from "@/components/layouts/Features";
import { AppSurfaces } from "@/components/layouts/AppSurfaces";
import { HowItWorks } from "@/components/layouts/HowItWorks";
import { Premium } from "@/components/layouts/Premium";
import { TechStack } from "@/components/layouts/TechStack";
import { GetStarted } from "@/components/layouts/GetStarted";
import { Footer } from "@/components/layouts/Footer";

type View = "hero" | "leaving" | "content";

// Must match the `duration-300` Tailwind class on the fade wrapper below.
const LEAVE_DURATION_MS = 300;

/**
 * Landing page where the hero acts as a one-time welcome screen. Clicking
 * Explore fades the hero out, then swaps in the site content, the hero is
 * unmounted at that point (not just scrolled past), so it can't be scrolled
 * back into.
 *
 * The URL is kept in sync with `view` (`/` for hero, `/?view=content` for
 * content) via a `replace` navigation, so the browser's real back button
 * lands on the right one: clicking a feature card pushes `/docs/<slug>` on
 * top of whichever URL matched the view you were actually looking at, so
 * `navigate(-1)` (the docs back button) pops back to it correctly instead
 * of always landing on the bare `/` and re-triggering the hero.
 */
export function Home() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const [view, setView] = useState<View>(
    searchParams.get("view") === "content" ? "content" : "hero",
  );
  const isFirstRender = useRef(true);

  useEffect(() => {
    if (view !== "leaving") return;
    const timer = setTimeout(() => setView("content"), LEAVE_DURATION_MS);
    return () => clearTimeout(timer);
  }, [view]);

  useEffect(() => {
    if (isFirstRender.current) {
      isFirstRender.current = false;
      return;
    }
    if (view === "content") {
      navigate("/?view=content", { replace: true });
    } else if (view === "hero") {
      navigate("/", { replace: true });
    }
  }, [view, navigate]);

  if (view === "content") {
    return (
      <div className="min-h-screen bg-elev">
        <Navbar onLogoClick={() => setView("hero")} />
        <Features />
        <AppSurfaces />
        <HowItWorks />
        <Premium />
        <GetStarted />
        <TechStack />
        <Footer />
      </div>
    );
  }

  return (
    <div
      className={`bg-side transition-opacity duration-300 ${view === "leaving" ? "opacity-0" : "opacity-100"}`}
    >
      <Hero onExplore={() => setView("leaving")} />
    </div>
  );
}
