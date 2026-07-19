import { describe, expect, it } from "bun:test";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const root = join(import.meta.dir, "..");

describe("flux pack integrity matrix (empty-registry P0 regression)", () => {
  it("pack integrity gate passes (real build+pack)", () => {
    const r = spawnSync("bash", ["scripts/check-flux-pack-integrity.sh"], {
      cwd: root,
      encoding: "utf8",
      env: { ...process.env, SCRATCH_DIR: join(root, ".pack-integrity-test") },
    });
    expect(r.status).toBe(0);
    expect(r.stdout).toContain("OK: flux-wasm pack integrity");
  }, 300_000);
});
