"use client";

import { useQuery } from "@tanstack/react-query";
import { serviceClient } from "@/lib/api/service-client";

export const requestLogBodiesQueryKey = (
  serviceAddr: string,
  traceId: string
) => ["logs", "bodies", serviceAddr, traceId] as const;

export function useRequestLogBodies(
  serviceAddr: string,
  traceId: string,
  enabled: boolean
) {
  return useQuery({
    queryKey: requestLogBodiesQueryKey(serviceAddr, traceId),
    queryFn: () => serviceClient.readRequestLogBodies(traceId),
    enabled: enabled && Boolean(traceId.trim()),
    staleTime: Infinity,
    retry: 1,
  });
}
