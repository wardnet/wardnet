import { useNavigate } from "react-router";
import { Button } from "@wardnet/web";
import { Heading, Text } from "@wardnet/web";

/** 404 page shown for unknown routes. Renders as a full-page Forge `.empty` block. */
export default function NotFound() {
  const navigate = useNavigate();

  return (
    <div
      className="empty col items-center justify-center gap-4"
      data-testid="notfound-page"
    >
      <Text
        as="p"
        weight="semibold"
        className="text-6xl tracking-tight text-ink"
      >
        404
      </Text>
      <Heading level={2} size="3xl" className="text-ink">
        Page not found
      </Heading>
      <Text as="p" size="sm" className="mt-1 max-w-md text-ink-3">
        The page you're looking for doesn't exist or has been moved.
      </Text>
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
