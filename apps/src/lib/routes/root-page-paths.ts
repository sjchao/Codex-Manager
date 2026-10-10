export const ROOT_PAGE_PATHS = [
  "/",
  "/accounts",
  "/aggregate-api",
  "/apikeys",
  "/plugins",
  "/qoder",
  "/logs",
  "/settings",
] as const;

export type RootPagePath = (typeof ROOT_PAGE_PATHS)[number];
