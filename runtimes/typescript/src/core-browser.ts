import initWasm, * as wasm from "../core/browser/sahou_core.js";
import type { CoreModule } from "./core.js";

/** wasm core (web target, requires init). The .wasm is resolved through a bundler (e.g. vite). */
export async function loadCore(): Promise<CoreModule> {
  await initWasm();
  return wasm as unknown as CoreModule;
}
