import { describe, expect, it } from "bun:test";
import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const root = join(import.meta.dir, "..");

describe("flux pack integrity matrix (empty-registry P0 regression)", () => {
  it("build strips wasm-pack .gitignore and prepack is fail-closed", () => {
    const pkg = JSON.parse(readFileSync(join(root, "packages/flux-wasm/package.json"), "utf8"));
    expect(JSON.stringify(pkg.scripts || {})).toContain("rm -f");
    expect(JSON.stringify(pkg.scripts || {})).toMatch(/flux_wasm_bg\.wasm|\.wasm/);
  });

  it("check-flux-pack-integrity script enforces non-empty wasm in tarball", () => {
    const script = readFileSync(join(root, "scripts/check-flux-pack-integrity.sh"), "utf8");
    expect(script).toContain("flux_wasm_bg.wasm");
    expect(script).toContain(".gitignore");
    expect(script).toContain("npm pack");
  });

  it("ledger is fully ts_deleted", () => {
    const ledger = JSON.parse(
      readFileSync(join(root, "docs/specs/migration-ledger.json"), "utf8"),
    ) as { capabilities: Array<{ state: string }>; summary: { ts_deleted: number } };
    for (const c of ledger.capabilities) expect(c.state).toBe("ts_deleted");
    expect(ledger.summary.ts_deleted).toBe(ledger.capabilities.length);
  });

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
