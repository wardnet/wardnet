import { useNavigate } from "react-router";
import { Button } from "@wardnet/web";

/** 404 page shown for unknown routes. Renders as a full-page Forge `.empty` block. */
export default function NotFound() {
  const navigate = useNavigate();

  return (
    <div
      className="empty col items-center justify-center gap-4"
      data-testid="notfound-page"
    >
      <p className="h-title text-6xl">404</p>
      <h2 className="h-title">Page not found</h2>
      <p className="h-sub max-w-md">
        The page you're looking for doesn't exist or has been moved.
      </p>
      <Button
        variant="outline"
        onClick={() => navigate("/")}
        data-testid="notfound-home"
      >
        Go to Dashboard
      </Button>
    </div>
  );
}
