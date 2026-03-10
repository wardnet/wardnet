import { useState } from "react";
import { useSearchParams } from "react-router";
import { Hero } from "@/components/layouts/Hero";
import { Navbar } from "@/components/layouts/Navbar";
import { Features } from "@/components/layouts/Features";
import { HowItWorks } from "@/components/layouts/HowItWorks";
import { TechStack } from "@/components/layouts/TechStack";
import { GetStarted } from "@/components/layouts/GetStarted";
import { Footer } from "@/components/layouts/Footer";

/**
 * Landing page where the hero acts as a welcome screen.
 * Once the user clicks Explore, the hero is dismissed and the content takes over.
 * Navigating to /?view=content skips the hero (used by the docs back button).
 */
export function Home() {
  const [searchParams] = useSearchParams();
  const [showHero, setShowHero] = useState(searchParams.get("view") !== "content");

  if (!showHero) {
    return (
      <div className="min-h-screen bg-white dark:bg-[oklch(0.13_0.02_270)]">
        <Navbar onLogoClick={() => setShowHero(true)} />
        <Features />
        <HowItWorks />
        <TechStack />
        <GetStarted />
        <Footer />
      </div>
    );
  }

  return <Hero onExplore={() => setShowHero(false)} />;
}
