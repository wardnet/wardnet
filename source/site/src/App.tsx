import { BrowserRouter, Routes, Route } from "react-router";
import { Home } from "@/pages/Home";
import { Docs } from "@/pages/Docs";

/**
 * Root application component with routing for the public site.
 */
export default function App() {
  return (
    <BrowserRouter basename={import.meta.env.BASE_URL}>
      <Routes>
        <Route path="/" element={<Home />} />
        <Route path="/docs" element={<Docs />} />
      </Routes>
    </BrowserRouter>
  );
}
