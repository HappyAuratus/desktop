import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import path from "node:path";

export default defineConfig({
  plugins: [react()],
  // Keep progress on stdout; the default TTY reporter clears the screen and
  // can hide stderr that run-with-clean-stderr is gating on.
  clearScreen: false,
  resolve: {
    alias: {
      "@ora/editor/composer": path.resolve(
        __dirname,
        "../editor/src/composer/index.ts",
      ),
      "@ora/editor": path.resolve(__dirname, "../editor/src/index.ts"),
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    css: false,
    // Every test file imports the whole app graph (~15s of module evaluation
    // per file), so the suite is import-bound. One worker per logical core
    // saturates the machine and, under that saturation, wall-clock budgets
    // (waitFor polling, the test timeout) breach stochastically — healthy
    // tests time out, and a timed-out typing chain keeps dispatching into
    // the next test's editor. Halve the workers so each gets a full physical
    // core and worker event loops stay responsive.
    maxWorkers: "50%",
    // Timeouts exist to catch hangs, not to benchmark machine load: the
    // slowest healthy tests take ~2s unloaded but stretch several-fold under
    // parallel import/transform pressure. 20s is ~10x margin over the worst
    // real test, which keeps load spikes from failing healthy tests while
    // still catching genuine infinite loops.
    testTimeout: 20_000,
    hookTimeout: 20_000,
  },
});
