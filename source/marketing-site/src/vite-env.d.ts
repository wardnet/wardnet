/// <reference types="vite/client" />

declare module "*.yml" {
  const data: Record<string, unknown>;
  export default data;
}
