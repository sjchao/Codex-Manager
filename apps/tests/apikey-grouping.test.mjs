import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");
const sourcePath = path.join(appsRoot, "src", "app", "apikeys", "grouping.ts");

async function loadGroupingModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
      baseUrl: appsRoot,
      paths: {
        "@/*": ["src/*"],
      },
    },
    fileName: sourcePath,
  });

  const tempDir = await fs.mkdtemp(
    path.join(os.tmpdir(), "codexmanager-apikey-grouping-")
  );
  const tempFile = path.join(tempDir, "grouping.mjs");
  await fs.writeFile(tempFile, compiled.outputText, "utf8");
  return import(pathToFileURL(tempFile).href);
}

const grouping = await loadGroupingModule();

test("buildApiKeyGroupOptions 返回全部、已命名分组与未分组", () => {
  const apiKeys = [
    { id: "1", groupName: "生产" },
    { id: "2", groupName: "测试" },
    { id: "3", groupName: "生产" },
    { id: "4", groupName: "" },
  ];

  const options = grouping.buildApiKeyGroupOptions(apiKeys);

  assert.deepEqual(options, [
    { value: "__all__", label: "全部", count: 4 },
    { value: "测试", label: "测试", count: 1 },
    { value: "生产", label: "生产", count: 2 },
    { value: "__ungrouped__", label: "未分组", count: 1 },
  ]);
});

test("filterApiKeysByGroup 支持全部、具体分组与未分组", () => {
  const apiKeys = [
    { id: "1", groupName: "生产" },
    { id: "2", groupName: "测试" },
    { id: "3", groupName: "" },
  ];

  assert.equal(
    grouping.filterApiKeysByGroup(apiKeys, grouping.ALL_API_KEY_GROUP_VALUE).length,
    3
  );
  assert.deepEqual(
    grouping.filterApiKeysByGroup(apiKeys, "生产").map((item) => item.id),
    ["1"]
  );
  assert.deepEqual(
    grouping
      .filterApiKeysByGroup(apiKeys, grouping.UNGROUPED_API_KEY_GROUP_VALUE)
      .map((item) => item.id),
    ["3"]
  );
});
