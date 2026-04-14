import { useNavigate } from "react-router";
import { Button } from "@/components/core/ui/button";

/** 404 page shown for unknown routes. */
export default function NotFound() {
  const navigate = useNavigate();

  return (
    <div className="flex flex-col items-center justify-center gap-4 py-20">
      <p className="text-6xl font-bold text-muted-foreground/30">404</p>
      <h2 className="text-xl font-semibold">Page not found</h2>
      <p className="text-sm text-muted-foreground">
        The page you're looking for doesn't exist or has been moved.
      </p>
      <Button variant="outline" onClick={() => navigate("/")}>
        Go to Dashboard
      </Button>
    </div>
  );
}
