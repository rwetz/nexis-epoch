import { describe, expect, it } from "vitest";
import type { mountEpoch as Declared } from "./embed.d";

// Compile-time: the implementation must stay assignable to the declaration
// hosts type-check against. `pnpm typecheck` fails if they drift.
type Implemented = typeof import("./embed").mountEpoch;
const sameShape: (x: Implemented) => typeof Declared = (x) => x;

describe("embed.d.ts", () => {
  it("matches embed.tsx", () => {
    expect(typeof sameShape).toBe("function");
  });
});
