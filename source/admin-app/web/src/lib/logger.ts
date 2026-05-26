import { createLogger } from "@wardnet/js";

export { createLogger, setLevel } from "@wardnet/js";
export type { Logger, LogLevel } from "@wardnet/js";

export const logger = createLogger("wardnet.ui");
