"use client";

import { useQuery } from "@tanstack/react-query";
import { serviceClient } from "@/lib/api/service-client";

export const requestLogImagesQueryKey = (serviceAddr: string, traceId: string) =>
  ["logs", "images", serviceAddr, traceId] as const;

export function useRequestLogImages(
  serviceAddr: string,
  traceId: string,
  enabled: boolean
) {
  return useQuery({
    queryKey: requestLogImagesQueryKey(serviceAddr, traceId),
    queryFn: () => serviceClient.readRequestLogImages(traceId),
    enabled: enabled && Boolean(traceId.trim()),
    staleTime: Infinity,
    retry: 1,
  });
}
