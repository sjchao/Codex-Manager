import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import ts from "../node_modules/typescript/lib/typescript.js";

const appsRoot = path.resolve(import.meta.dirname, "..");
const sourcePath = path.join(
  appsRoot,
  "src",
  "lib",
  "runtime",
  "runtime-capabilities.ts"
);
const normalizeSourcePath = path.join(
  appsRoot,
  "src",
  "lib",
  "api",
  "normalize.ts"
);
const usageSourcePath = path.join(
  appsRoot,
  "src",
  "lib",
  "utils",
  "usage.ts"
);
const transportSourcePath = path.join(
  appsRoot,
  "src",
  "lib",
  "api",
  "transport.ts"
);

/**
 * 函数 `loadRuntimeModule`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * 无
 *
 * # 返回
 * 返回函数执行结果
 */
async function loadRuntimeModule() {
  const source = await fs.readFile(sourcePath, "utf8");
  const compiled = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
    },
    fileName: sourcePath,
  });

  const tempDir = await fs.mkdtemp(
    path.join(os.tmpdir(), "codexmanager-runtime-capabilities-")
  );
  const tempFile = path.join(tempDir, "runtime-capabilities.mjs");
  await fs.writeFile(tempFile, compiled.outputText, "utf8");
  return import(pathToFileURL(tempFile).href);
}

async function loadNormalizeModule() {
  const [normalizeSource, usageSource] = await Promise.all([
    fs.readFile(normalizeSourcePath, "utf8"),
    fs.readFile(usageSourcePath, "utf8"),
  ]);
  const tempDir = await fs.mkdtemp(
    path.join(os.tmpdir(), "codexmanager-requestlog-images-")
  );
  const normalizeFile = path.join(tempDir, "normalize.mjs");
  const usageFile = path.join(tempDir, "usage.mjs");
  const compilerOptions = {
    module: ts.ModuleKind.ES2022,
    target: ts.ScriptTarget.ES2022,
  };
  const normalizedSource = normalizeSource.replace(
    'from "@/lib/utils/usage"',
    'from "./usage.mjs"'
  );

  await Promise.all([
    fs.writeFile(
      normalizeFile,
      ts.transpileModule(normalizedSource, {
        compilerOptions,
        fileName: normalizeSourcePath,
      }).outputText,
      "utf8"
    ),
    fs.writeFile(
      usageFile,
      ts.transpileModule(usageSource, {
        compilerOptions,
        fileName: usageSourcePath,
      }).outputText,
      "utf8"
    ),
  ]);

  return import(pathToFileURL(normalizeFile).href);
}

const runtime = await loadRuntimeModule();
const normalize = await loadNormalizeModule();

test("normalizeRuntimeCapabilities 为 Web 网关补齐默认能力", () => {
  const capabilities = runtime.normalizeRuntimeCapabilities(
    {
      mode: "web-gateway",
      rpcBaseUrl: "/gateway/rpc/",
    },
    "/api/rpc"
  );

  assert.equal(capabilities.mode, "web-gateway");
  assert.equal(capabilities.rpcBaseUrl, "/gateway/rpc");
  assert.equal(capabilities.canManageService, false);
  assert.equal(capabilities.canUseBrowserFileImport, true);
  assert.equal(capabilities.canUseBrowserDownloadExport, true);
});

test("normalizeRuntimeCapabilities 在 unsupported-web 下保持保守默认值", () => {
  const capabilities = runtime.normalizeRuntimeCapabilities(
    {
      mode: "unsupported-web",
    },
    "/proxy/rpc"
  );

  assert.equal(capabilities.mode, "unsupported-web");
  assert.equal(capabilities.rpcBaseUrl, "/proxy/rpc");
  assert.equal(capabilities.canManageService, false);
  assert.equal(capabilities.canUseBrowserFileImport, false);
  assert.equal(capabilities.canUseBrowserDownloadExport, false);
  assert.match(capabilities.unsupportedReason, /CodexManager Web 运行壳/);
});

test("normalizeRuntimeCapabilities 在未知 mode 下回退到 web-gateway", () => {
  const capabilities = runtime.normalizeRuntimeCapabilities(
    {
      mode: "legacy-web",
      rpcBaseUrl: "",
      canSelfUpdate: true,
    },
    "/custom/rpc"
  );

  assert.equal(capabilities.mode, "web-gateway");
  assert.equal(capabilities.rpcBaseUrl, "/custom/rpc");
  assert.equal(capabilities.canSelfUpdate, true);
});

test("resolveRuntimeCapabilityView 在桌面回退路径下暴露桌面能力", () => {
  const view = runtime.resolveRuntimeCapabilityView(null, true);

  assert.equal(view.mode, "desktop-tauri");
  assert.equal(view.isDesktopRuntime, true);
  assert.equal(view.canAccessManagementRpc, true);
  assert.equal(view.canManageService, true);
  assert.equal(view.canSelfUpdate, true);
  assert.equal(view.canOpenLocalDir, true);
});

test("resolveRuntimeCapabilityView 在未探测到运行壳前保持 Web 保守模式", () => {
  const view = runtime.resolveRuntimeCapabilityView(null, false);

  assert.equal(view.mode, "unsupported-web");
  assert.equal(view.isUnsupportedWebRuntime, true);
  assert.equal(view.canAccessManagementRpc, false);
  assert.equal(view.canManageService, false);
  assert.equal(view.canUseBrowserFileImport, false);
  assert.equal(view.canUseBrowserDownloadExport, false);
});

test("resolveRuntimeCapabilityView 直接复用已探测到的 Web 网关能力", () => {
  const capabilities = runtime.buildWebGatewayRuntimeCapabilities("/managed/rpc");
  const view = runtime.resolveRuntimeCapabilityView(capabilities, false);

  assert.equal(view.mode, "web-gateway");
  assert.equal(view.isDesktopRuntime, false);
  assert.equal(view.canAccessManagementRpc, true);
  assert.equal(view.canManageService, false);
  assert.equal(view.canUseBrowserFileImport, true);
  assert.equal(view.canUseBrowserDownloadExport, true);
});

test("normalizeRequestLog 保留合法图片结果并忽略畸形元数据", () => {
  const normalized = normalize.normalizeRequestLog({
    traceId: "trace-image",
    method: "POST",
    requestPath: "/v1/images/generations",
    imageResults: [
      {
        storageKey: "trace-image/0.png",
        mimeType: "image/png",
        byteLength: 12,
      },
      {
        storageKey: "",
        mimeType: "image/png",
        byteLength: 12,
      },
      {
        storageKey: "trace-image/1.png",
        mimeType: "image/png",
        byteLength: -1,
      },
    ],
  });

  assert.deepEqual(normalized?.imageResults, [
    {
      storageKey: "trace-image/0.png",
      mimeType: "image/png",
      byteLength: 12,
    },
  ]);
});

test("normalizeRequestLogImageData 只保留可用的数据 URL", () => {
  const normalized = normalize.normalizeRequestLogImageData([
    {
      storageKey: "trace-image/0.png",
      dataUrl: "data:image/png;base64,cG5n",
    },
    {
      storageKey: "trace-image/1.png",
      dataUrl: "https://upstream.example/image.png",
    },
    {
      storageKey: "",
      dataUrl: "data:image/png;base64,cG5n",
    },
  ]);

  assert.deepEqual(normalized, [
    {
      storageKey: "trace-image/0.png",
      dataUrl: "data:image/png;base64,cG5n",
    },
  ]);
});

test("Web transport exposes request-log image reads through the shared RPC command", async () => {
  const source = await fs.readFile(transportSourcePath, "utf8");

  assert.match(
    source,
    /service_requestlog_images_read:\s*\{\s*rpcMethod:\s*"requestlog\/images\/read"\s*\}/
  );
});
