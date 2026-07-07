import { Navbar } from "@/components/layouts/Navbar";
import { Premium as PremiumSection } from "@/components/layouts/Premium";
import { AppSurfaces } from "@/components/layouts/AppSurfaces";
import { Footer } from "@/components/layouts/Footer";

/**
 * Standalone Premium page reached from the "Premium" nav entry. Reuses the
 * homepage Premium section (the qualitative Free-vs-Premium story with the
 * `account.wardnet.network` CTA) and the app-surfaces showcase so the
 * remote/mobile story stands on its own URL.
 */
export function Premium() {
  return (
    <div className="min-h-screen bg-elev">
      <Navbar showBack />
      <PremiumSection />
      <AppSurfaces />
      <Footer />
    </div>
  );
}
