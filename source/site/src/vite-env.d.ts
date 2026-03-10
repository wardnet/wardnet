/// <reference types="vite/client" />

declare module "@fontsource-variable/geist";

declare module "*.yml" {
  const data: Record<string, unknown>;
  export default data;
}
