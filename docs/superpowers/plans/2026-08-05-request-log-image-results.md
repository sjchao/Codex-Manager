# Request Log Image Results Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist successful image-generation results beneath the instance data directory, show thumbnails inline in image request logs across Web and Tauri, and delete the files when request logs are cleared.

**Architecture:** The HTTP response bridge captures only buffered, successful image responses and delegates `b64_json` and signed URL persistence to a shared service module. SQLite keeps small ordered asset metadata in `request_logs.image_results_json`; the binary files remain under `<db-parent>/request-log-images/`. Web and Tauri read image data lazily through the same authenticated RPC command, so no data-directory file is publicly routable.

**Tech Stack:** Rust 2021, rusqlite migrations, reqwest blocking client, tiny_http bridge, Tauri v2 commands, Next.js/React 19, TanStack Query v5, shadcn Dialog, Tailwind CSS v4.

---

## File Map

- Create: `crates/core/migrations/052_request_log_image_results.sql` -- add nullable `image_results_json` for image asset metadata.
- Modify: `crates/core/src/storage/mod.rs` -- register migration `052_request_log_image_results` and add the `RequestLog.image_results_json` storage field.
- Modify: `crates/core/src/storage/request_logs.rs` -- write and hydrate the new column in every request-log INSERT and SELECT projection.
- Modify: `crates/core/src/rpc/types.rs` -- define serialized `RequestLogImageResult`, `RequestLogImageData`, and `RequestLogImageReadParams`; expose `image_results` on `RequestLogSummary`.
- Create: `crates/service/src/requestlog/image_assets.rs` -- own filesystem path derivation, JSON response extraction, bounded local writes, safe reads, and clear-time deletion.
- Modify: `crates/service/src/requestlog/mod.rs` -- expose `image_assets` to request-log readers, clear logic, and gateway observability.
- Modify: `crates/service/src/gateway/observability/http_bridge/aggregate/output_text.rs` -- carry optional serialized image-result metadata in `UpstreamResponseBridgeResult`.
- Modify: `crates/service/src/gateway/observability/http_bridge/mod.rs` and `delivery.rs` -- accept an image-capture flag and cache images after a successful non-stream response has been delivered.
- Modify: `crates/service/src/gateway/upstream/proxy_pipeline/response_finalize.rs`, `upstream/protocol/aggregate_api.rs`, and `upstream/protocol/azure_openai.rs` -- pass image-request context to the bridge and forward its result into the final request log.
- Modify: `crates/service/src/gateway/observability/request_log.rs` -- persist bridge image metadata via `RequestLogTraceContext` without changing request success semantics.
- Modify: `crates/service/src/requestlog/requestlog_list.rs`, `requestlog_clear.rs`, and `rpc_dispatch/requestlog.rs` -- map metadata for list results, add `requestlog/images/read`, and make clear delete files before records.
- Modify: `crates/service/src/gateway/observability/tests/http_bridge_tests.rs`, `crates/core/tests/storage.rs`, and `crates/service/tests/rpc.rs` -- cover bridge capture, migration/list mapping, authenticated RPC reads, and clear behavior.
- Modify: `apps/src-tauri/src/commands/requestlog.rs` and `apps/src-tauri/src/commands/registry.rs` -- expose `service_requestlog_images_read` to desktop IPC.
- Modify: `apps/src/types/index.ts`, `apps/src/lib/api/normalize.ts`, `apps/src/lib/api/service-client.ts`, and `apps/src/lib/api/transport.ts` -- expose normalized asset metadata and map the new command in both runtime transports.
- Create: `apps/src/hooks/useRequestLogImages.ts` -- centralize the lazy TanStack Query image-data request and its stable query key.
- Create: `apps/src/components/logs/request-log-image-result-cell.tsx` -- render fixed-size inline thumbnails with a loading and missing-result state.
- Modify: `apps/src/app/logs/page.tsx` -- add the image-result column only in the image tab, host the full-image dialog, and evict preview query cache after clearing logs.

### Task 1: Add the Storage Contract

**Files:**
- Create: `crates/core/migrations/052_request_log_image_results.sql`
- Modify: `crates/core/src/storage/mod.rs`
- Modify: `crates/core/src/storage/request_logs.rs`
- Modify: `crates/core/src/rpc/types.rs`
- Test: `crates/core/tests/storage.rs`
- Test: `crates/core/src/rpc/tests/types_tests.rs`

- [ ] **Step 1: Write the failing storage and serialization tests**

Add a request log whose `image_results_json` is `[{
  "storageKey":"trace-1/0.png","mimeType":"image/png","byteLength":12
}]`, then assert that `list_request_logs` returns the exact JSON. Add an RPC serialization assertion that `RequestLogSummary.image_results[0]` serializes as `storageKey`, `mimeType`, and `byteLength`.

```rust
assert_eq!(logs[0].image_results_json.as_deref(), Some(image_results_json));
assert_eq!(payload["imageResults"][0]["storageKey"], "trace-1/0.png");
```

- [ ] **Step 2: Run the focused tests and verify they fail for the missing field**

Run: `cargo test -p codexmanager-core request_log_image_results`

Expected: compile failure because `RequestLog.image_results_json` and `RequestLogSummary.image_results` do not exist.

- [ ] **Step 3: Add migration and Rust types**

Create migration `052_request_log_image_results.sql`:

```sql
ALTER TABLE request_logs
  ADD COLUMN image_results_json TEXT;
```

Register it after migration `051` in `Storage::init`. Define these types in `rpc/types.rs` and make `image_results` default to an empty vector:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestLogImageResult {
    pub storage_key: String,
    pub mime_type: String,
    pub byte_length: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestLogImageData {
    pub storage_key: String,
    pub data_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestLogImageReadParams {
    pub trace_id: String,
}
```

Add `image_results_json: Option<String>` to `RequestLog` and `image_results: Vec<RequestLogImageResult>` to `RequestLogSummary`.

- [ ] **Step 4: Thread the column through SQL without changing existing filter behavior**

Add `image_results_json` after `image_size` in `insert_request_log`, `insert_request_log_with_token_stat`, every request-log SELECT list, and `map_request_log_row`. Keep all existing filter clauses and index definitions unchanged. Initialize existing rows as `NULL`; `requestlog_list` will deserialize that value as `Vec::new()`.

- [ ] **Step 5: Run focused Core tests and the migration suite**

Run: `cargo test -p codexmanager-core request_log_image_results`

Expected: PASS. Then run: `cargo test -p codexmanager-core storage::migration_tests`

Expected: PASS with the new migration applied idempotently.

### Task 2: Build Safe Image Asset Storage

**Files:**
- Create: `crates/service/src/requestlog/image_assets.rs`
- Modify: `crates/service/src/requestlog/mod.rs`
- Test: `crates/service/src/requestlog/image_assets.rs` (`#[cfg(test)]` module)

- [ ] **Step 1: Write failing unit tests for directory resolution, base64 persistence, URL persistence, and path rejection**

Use a temporary database path such as `temp/codexmanager.db`. Assert that a PNG `b64_json` response creates `temp/request-log-images/trace-image/` and produces metadata with a relative storage key. Start a `tiny_http::Server` that returns `image/png` bytes and assert the `data[].url` case creates a second file. Assert `../outside.png` and absolute storage keys are rejected by the reader.

```rust
assert!(root.join(&assets[0].storage_key).is_file());
assert!(read_image_data_urls(&db_path, "../outside").is_err());
```

- [ ] **Step 2: Run the unit tests and verify they fail because the module is absent**

Run: `cargo test -p codexmanager-service requestlog::image_assets`

Expected: compile failure because `image_assets` is not declared.

- [ ] **Step 3: Implement bounded, atomic image caching**

Implement these public-to-service functions:

```rust
pub(crate) fn cache_openai_image_results(
    db_path: &Path,
    trace_id: &str,
    response_body: &[u8],
) -> Vec<RequestLogImageResult>;

pub(crate) fn read_image_data_urls(
    db_path: &Path,
    trace_id: &str,
    image_results_json: Option<&str>,
) -> Result<Vec<RequestLogImageData>, String>;

pub(crate) fn clear_image_results(
    db_path: &Path,
    image_results_jsons: impl IntoIterator<Item = Option<String>>,
) -> Result<(), String>;
```

Derive the root as `db_path.parent().unwrap_or_else(|| Path::new(".")).join("request-log-images")`. Create only a trace-specific directory below that root. Accept decoded `data[].b64_json` and `data[].url`; enforce `http`/`https`, a 10-second blocking-client timeout, 20 MiB per image, and 64 MiB per response. Detect PNG, JPEG, WebP, and GIF bytes before committing a temporary file with `rename`. Do not send credentials, request headers, or cookies when downloading a response URL. Log and skip individual failures.

- [ ] **Step 4: Implement safe reads and all-or-error clearing**

Require every storage key to consist only of normal relative path components and verify `root.join(storage_key).starts_with(&root)` after normalization. `read_image_data_urls` must return `data:<mime>;base64,<payload>` only for metadata belonging to the supplied trace ID. During clear, treat missing files as already removed; any other filesystem failure returns an error before request-log records are deleted.

- [ ] **Step 5: Run the asset module tests**

Run: `cargo test -p codexmanager-service requestlog::image_assets`

Expected: PASS for automatic directory creation, both source formats, size/path rejection, and missing-file cleanup.

### Task 3: Attach Cached Results to Final Gateway Logs

**Files:**
- Modify: `crates/service/src/gateway/observability/http_bridge/aggregate/output_text.rs`
- Modify: `crates/service/src/gateway/observability/http_bridge/mod.rs`
- Modify: `crates/service/src/gateway/observability/http_bridge/delivery.rs`
- Modify: `crates/service/src/gateway/observability/request_log.rs`
- Modify: `crates/service/src/gateway/upstream/proxy_pipeline/response_finalize.rs`
- Modify: `crates/service/src/gateway/upstream/protocol/aggregate_api.rs`
- Modify: `crates/service/src/gateway/upstream/protocol/azure_openai.rs`
- Test: `crates/service/src/gateway/observability/tests/http_bridge_tests.rs`

- [ ] **Step 1: Write failing bridge regression tests**

Build a non-streaming successful `ResponseAdapter::Passthrough` upstream response containing `{"data":[{"b64_json":"..."}]}`. Assert the bridge result contains non-empty image metadata when `capture_image_results` is true, but returns an empty result for a non-image request, 4xx response, stream response, and invalid JSON. Assert the bytes delivered to the original tiny_http client equal the source response byte-for-byte.

```rust
assert!(!bridge.image_results_json.as_deref().unwrap_or_default().is_empty());
assert_eq!(delivered_body, upstream_body);
```

- [ ] **Step 2: Run the bridge test and verify the capture field is missing**

Run: `cargo test -p codexmanager-service http_bridge_caches_successful_image_response`

Expected: compile failure because `UpstreamResponseBridgeResult.image_results_json` and `capture_image_results` do not exist.

- [ ] **Step 3: Add bridge metadata and cache only after delivery succeeds**

Add `image_results_json: Option<String>` to `UpstreamResponseBridgeResult`. Extend `respond_with_upstream` in `http_bridge/mod.rs` and `delivery.rs` with `capture_image_results: bool`. In the non-stream `Passthrough` JSON branch, call `request.respond(response)` first; only when the delivered status is 2xx, there is no delivery error, capture is enabled, and `trace_id` exists, call `cache_openai_image_results` using the already buffered body. Serialize non-empty assets to the new field. All existing early error, compact-response, SSE, and streaming results leave the field `None`.

- [ ] **Step 4: Propagate the flag and metadata through every final response path**

Pass `model_type == ModelType::Image` from `response_finalize.rs` and aggregate API flow. Pass `false` for Azure paths that do not have image-model request metadata. Add `image_results_json: bridge.image_results_json.as_deref()` to `RequestLogTraceContext`, then persist it in `write_request_log_with_attempts`. Preserve all current failover decisions: a cache failure does not create a failover or alter `status_for_log`.

- [ ] **Step 5: Run bridge and gateway log regression tests**

Run: `cargo test -p codexmanager-service http_bridge`

Expected: PASS. Then run: `cargo test -p codexmanager-service gateway_request_log`

Expected: PASS with one final request log per request and image metadata only for successful image responses.

### Task 4: Expose Metadata, Protected Image Reads, and Clear Semantics

**Files:**
- Modify: `crates/service/src/requestlog/requestlog_list.rs`
- Modify: `crates/service/src/requestlog/requestlog_clear.rs`
- Modify: `crates/service/src/rpc_dispatch/requestlog.rs`
- Test: `crates/service/tests/rpc.rs`
- Test: `crates/core/tests/storage.rs`

- [ ] **Step 1: Write failing RPC and clear tests**

Seed one image log with metadata and an on-disk PNG. Assert `requestlog/list` exposes only metadata, `requestlog/images/read` accepts the trace ID and returns the matching data URL, and `requestlog/clear` removes both the file and the request-log row while `summarize_request_logs_between` still returns the existing token statistics. Add a permissions/error test that makes image deletion fail and asserts the request log remains.

```rust
assert!(read_result["items"][0]["dataUrl"].as_str().unwrap().starts_with("data:image/png;base64,"));
assert!(!db_parent.join("request-log-images/trace-image/0.png").exists());
assert_eq!(summary.total_tokens, 100);
```

- [ ] **Step 2: Run the focused RPC tests and verify the new RPC is rejected**

Run: `cargo test -p codexmanager-service rpc_requestlog_images_read`

Expected: FAIL because `requestlog/images/read` is not registered.

- [ ] **Step 3: Map metadata and add the read RPC**

Deserialize `item.image_results_json` in `to_request_log_summary`, falling back to an empty vector on `NULL` or malformed historical data. Implement `read_request_log_images(params)` to find the trace ID from storage, then call `read_image_data_urls` with only that row's metadata. Register:

```rust
"requestlog/images/read" => {
    let params = req.params.clone()
        .map(serde_json::from_value::<RequestLogImageReadParams>)
        .transpose()
        .map(|value| value.unwrap_or_default())
        .map_err(|err| format!("invalid requestlog/images/read params: {err}"));
    super::value_or_error(params.and_then(requestlog_list::read_request_log_images))
}
```

- [ ] **Step 4: Make clear remove assets before rows**

In `requestlog_clear::clear_request_logs`, read all request logs, pass their `image_results_json` values to `clear_image_results`, and call `storage.clear_request_logs()` only if it succeeds. Keep the existing `request_token_stats` preservation behavior exactly as-is.

- [ ] **Step 5: Run the focused RPC and Core clear tests**

Run: `cargo test -p codexmanager-service rpc_requestlog_images_read`

Expected: PASS. Run: `cargo test -p codexmanager-core clear_request_logs_keeps_token_stats_for_usage_summary`

Expected: PASS.

### Task 5: Add Dual-Transport Client Access

**Files:**
- Modify: `apps/src-tauri/src/commands/requestlog.rs`
- Modify: `apps/src-tauri/src/commands/registry.rs`
- Modify: `apps/src/types/index.ts`
- Modify: `apps/src/lib/api/normalize.ts`
- Modify: `apps/src/lib/api/service-client.ts`
- Modify: `apps/src/lib/api/transport.ts`
- Test: `apps/tests/runtime-capabilities.test.mjs`

- [ ] **Step 1: Write failing normalizer and Web command-map tests**

Add a test payload containing `imageResults` metadata and a `requestlog/images/read` result containing `dataUrl`. Assert normalization retains only valid image entries and that the Web transport routes `service_requestlog_images_read` to the RPC method `requestlog/images/read`.

```javascript
assert.equal(normalized.imageResults[0].storageKey, "trace-image/0.png");
assert.equal(commandMap.service_requestlog_images_read.rpcMethod, "requestlog/images/read");
```

- [ ] **Step 2: Run the frontend test and verify it fails for the missing command**

Run: `pnpm -C apps run test:runtime`

Expected: FAIL because `service_requestlog_images_read` is absent from the command map and types.

- [ ] **Step 3: Add the Tauri command and Web command descriptor**

Add `service_requestlog_images_read(addr, trace_id)` in the request-log command module:

```rust
#[tauri::command]
pub async fn service_requestlog_images_read(
    addr: Option<String>,
    trace_id: String,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("requestlog/images/read", addr, Some(serde_json::json!({
        "traceId": trace_id
    }))).await
}
```

Register it next to the existing request-log commands. Add the Web descriptor and preserve `withAddr()` use through `serviceClient`:

```ts
service_requestlog_images_read: { rpcMethod: "requestlog/images/read" },
```

- [ ] **Step 4: Add normalized client types and service wrapper**

Add `RequestLogImageResult` and `RequestLogImageData` interfaces, parse `imageResults` in `normalizeRequestLog`, and add `serviceClient.readRequestLogImages(traceId)` that invokes `service_requestlog_images_read` with `withAddr({ traceId })`. Return an empty list for malformed entries rather than failing a logs page refresh.

- [ ] **Step 5: Run the frontend transport test and TypeScript build**

Run: `pnpm -C apps run test:runtime`

Expected: PASS. Run: `pnpm -C apps run build:desktop`

Expected: PASS with static export compatibility.

### Task 6: Render Inline Thumbnails and Full-Image Preview

**Files:**
- Create: `apps/src/hooks/useRequestLogImages.ts`
- Create: `apps/src/components/logs/request-log-image-result-cell.tsx`
- Modify: `apps/src/app/logs/page.tsx`
- Test: `apps/src/app/logs/page.tsx` via desktop build and manual rendered-state verification

- [ ] **Step 1: Add a failing compile-time use case for the new cell**

Temporarily import `RequestLogImageResultCell` in the image-log table and pass a `RequestLog` with `imageResults`. Run the desktop build before creating the component.

Run: `pnpm -C apps run build:desktop`

Expected: FAIL with `Cannot find module '@/components/logs/request-log-image-result-cell'`.

- [ ] **Step 2: Create the lazy query hook**

Create a stable query key and hook that reads only when a trace ID and metadata exist:

```ts
export const requestLogImagesQueryKey = (addr: string, traceId: string) =>
  ["logs", "images", addr, traceId] as const;

export function useRequestLogImages(addr: string, traceId: string, enabled: boolean) {
  return useQuery({
    queryKey: requestLogImagesQueryKey(addr, traceId),
    queryFn: () => serviceClient.readRequestLogImages(traceId),
    enabled,
    staleTime: Infinity,
    retry: 1,
  });
}
```

- [ ] **Step 3: Create the presentational image-result cell**

Render fixed `h-12 w-16` image buttons in a non-wrapping flex row. The cell receives `traceId`, metadata, `serviceAddr`, and `onPreview`; it uses the hook to obtain data URLs, renders skeletons while loading, and returns `-` when metadata or image data is absent. Each button uses `aria-label="查看第 N 张生成图片"` and calls `onPreview(dataUrl, storageKey)`.

- [ ] **Step 4: Integrate the column and dialog without changing non-image tabs**

In `logs/page.tsx`, add `previewedImage` state. When `modelTypeFilter === "image"`, add a `图片结果` header and one cell per row; increase table min width and loading/empty `colSpan` by one only in that tab. Use the existing shadcn `Dialog` to show the selected image with constrained viewport dimensions and `object-contain`. Keep all non-image table columns and row layout unchanged.

- [ ] **Step 5: Remove preview cache after a successful clear and verify visual states**

In the existing `clearMutation.onSuccess`, remove queries with `queryKey: ["logs", "images"]` in addition to invalidating logs and summaries, and close the preview dialog. Run `pnpm -C apps run build:desktop`.

Expected: PASS. Manually verify in both Web and Tauri: an image log displays fixed-size thumbnails, a click opens the source image, `-` appears for legacy/no-cache logs, and clearing immediately removes thumbnails and the open preview.

### Task 7: Run End-to-End Regression Verification

**Files:**
- Modify only if a verification failure reveals a defect in the files above.

- [ ] **Step 1: Run all targeted Rust tests**

Run:

```powershell
cargo test -p codexmanager-core request_log_image_results
cargo test -p codexmanager-core clear_request_logs_keeps_token_stats_for_usage_summary
cargo test -p codexmanager-service requestlog::image_assets
cargo test -p codexmanager-service http_bridge
cargo test -p codexmanager-service rpc_requestlog_images_read
```

Expected: all commands PASS.

- [ ] **Step 2: Run frontend and desktop packaging checks**

Run:

```powershell
pnpm -C apps run test:runtime
pnpm -C apps run build:desktop
cargo build --manifest-path apps/src-tauri/Cargo.toml
git diff --check
```

Expected: all commands exit 0.

- [ ] **Step 3: Verify a real local request and clear flow**

Start the service with a temporary `CODEXMANAGER_DB_PATH` under a `data/` directory. Send one `/v1/images/generations` request whose upstream test server returns `b64_json`, then verify all of the following:

```text
data/request-log-images/<trace-id>/ exists and contains a valid image
requestlog/list returns imageResults metadata but not a data URL
requestlog/images/read returns only that trace's data URL
requestlog/clear removes the file and list entry while usage summary remains
```

- [ ] **Step 4: Preserve the requested Git boundary**

Do not stage, commit, or push any change. Report the modified files and verification output to the user for an explicit commit decision.

## Plan Self-Review

- Spec coverage: Tasks 1-4 implement persisted metadata, automatic `data/request-log-images/` creation, `b64_json` and URL capture, protected reads, and clear-time deletion. Tasks 5-6 implement both runtime transports and the selected inline-thumbnail UI. Task 7 verifies Linux-style data placement, Tauri build compatibility, and the unchanged token-stat behavior.
- Placeholder scan: the plan contains no unresolved implementation markers; limits, directory name, request method, response types, and failure semantics are explicit.
- Type consistency: `RequestLogImageResult`, `RequestLogImageData`, `RequestLogImageReadParams`, `image_results_json`, `imageResults`, `requestlog/images/read`, and `service_requestlog_images_read` use the same names across migration, Rust RPC, Tauri, Web, and React tasks.
