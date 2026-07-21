import { createLogger, setAdapter } from "@wardnet/js";
import { createConsolaAdapter } from "@wardnet/js/consola";

// Route SDK + UI logging through consola for rich, tagged output. The SDK
// declares no hard dependency on consola (it defaults to a zero-dependency
// console adapter); the web apps bundle consola and opt in here, so every
// surface that imports `@wardnet/web` shares one configured adapter.
setAdapter(createConsolaAdapter());

export { createLogger, setLevel } from "@wardnet/js";
export type { Logger, LogLevel } from "@wardnet/js";

export const logger = createLogger("wardnet.ui");
