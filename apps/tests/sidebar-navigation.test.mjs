import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const sidebarSource = await readFile(
  new URL("../src/components/layout/sidebar.tsx", import.meta.url),
  "utf8",
);

test("sidebar navigation omits account and plugin entries", () => {
  assert.doesNotMatch(
    sidebarSource,
    /\{ name: "账号管理", href: "\/accounts\/", icon: Users \},/,
  );
  assert.doesNotMatch(
    sidebarSource,
    /\{ name: "插件中心", href: "\/plugins\/", icon: Puzzle \},/,
  );
});
