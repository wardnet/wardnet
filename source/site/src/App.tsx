import { BrowserRouter, Routes, Route } from "react-router";
import { Home } from "@/pages/Home";
import { Docs } from "@/pages/Docs";

/**
 * Root application component with routing for the public site.
 */
export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Home />} />
        <Route path="/docs" element={<Docs />} />
      </Routes>
    </BrowserRouter>
  );
}
