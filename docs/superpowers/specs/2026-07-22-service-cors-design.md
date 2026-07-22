# 48760 Service CORS Support

## Scope

Enable browser cross-origin access for the HTTP service listening on port 48760. The change applies to the Axum front proxy only and does not change API-key authentication, routing, upstream forwarding, or service bind settings.

## Policy

- Allow requests from any origin.
- Allow the HTTP methods and request headers required by browser API clients.
- Do not enable credentialed CORS requests. Cookie-based credentials remain unavailable across origins.
- Apply the policy to preflight requests, proxied API responses, and proxy-generated error responses.

## Implementation

Add Axum-compatible CORS middleware to the front proxy router in `crates/service/src/http/proxy_runtime.rs`. The middleware will own `OPTIONS` handling before requests enter the tiny_http backend, so preflight responses carry the required CORS headers consistently.

## Verification

Add router-level regression tests that make a real preflight request and a normal request. The tests will assert the expected CORS response headers and verify that the normal request still follows the existing proxy path.
