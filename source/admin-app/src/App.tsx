import { Route, Routes } from "react-router";

function Home() {
  return (
    <div className="flex min-h-screen items-center justify-center">
      <p className="text-lg font-medium">Wardnet Admin</p>
    </div>
  );
}

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Home />} />
    </Routes>
  );
}
