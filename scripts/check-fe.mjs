import { glob } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import process from "node:process";
import console from "node:console";

let failed = 0;
for await (const file of glob("src/**/*.check.ts")) {
  const r = spawnSync(process.execPath, [file], { stdio: "inherit" });
  if (r.status !== 0) {
    failed++;
    console.error(`FAIL ${file}`);
  }
}
process.exit(failed ? 1 : 0);
