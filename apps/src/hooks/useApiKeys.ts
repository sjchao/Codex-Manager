"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { accountClient } from "@/lib/api/account-client";
import {
  buildStartupSnapshotQueryKey,
  STARTUP_SNAPSHOT_REQUEST_LOG_LIMIT,
} from "@/lib/api/startup-snapshot";
import { getAppErrorMessage } from "@/lib/api/transport";
import { useDeferredDesktopActivation } from "@/hooks/useDeferredDesktopActivation";
import { useRuntimeCapabilities } from "@/hooks/useRuntimeCapabilities";
import { useAppStore } from "@/lib/store/useAppStore";
import { StartupSnapshot } from "@/types";

type ApiKeyPayload = Parameters<typeof accountClient.createApiKey>[0];

/**
 * 函数 `useApiKeys`
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
export function useApiKeys() {
  const queryClient = useQueryClient();
  const serviceStatus = useAppStore((state) => state.serviceStatus);
  const { canAccessManagementRpc } = useRuntimeCapabilities();
  const isServiceReady = canAccessManagementRpc && serviceStatus.connected;
  const areApiKeyQueriesEnabled = useDeferredDesktopActivation(isServiceReady);
  const startupSnapshot = queryClient.getQueryData<StartupSnapshot>(
    buildStartupSnapshotQueryKey(
      serviceStatus.addr,
      STARTUP_SNAPSHOT_REQUEST_LOG_LIMIT
    )
  );
  const startupApiKeys = startupSnapshot?.apiKeys || [];
  const startupApiModels = startupSnapshot?.apiModelOptions || [];
  const hasStartupApiKeySnapshot = startupApiKeys.length > 0;
  const hasStartupApiModelSnapshot = startupApiModels.length > 0;

  /**
   * 函数 `ensureServiceReady`
   *
   * 作者: gaohongshun
   *
   * 时间: 2026-04-02
   *
   * # 参数
   * - actionLabel: 参数 actionLabel
   *
   * # 返回
   * 返回函数执行结果
   */
  const ensureServiceReady = (actionLabel: string): boolean => {
    if (isServiceReady) {
      return true;
    }
    toast.info(`服务未连接，暂时无法${actionLabel}`);
    return false;
  };

  const apiKeysQuery = useQuery({
    queryKey: ["apikeys"],
    queryFn: () => accountClient.listApiKeys(),
    enabled: areApiKeyQueriesEnabled,
    retry: 1,
    placeholderData: (previousData) =>
      previousData || (startupApiKeys.length > 0 ? startupApiKeys : undefined),
  });

  const modelsQuery = useQuery({
    queryKey: ["apikey-models"],
    queryFn: () => accountClient.listModels(false),
    enabled: areApiKeyQueriesEnabled,
    retry: 1,
    placeholderData: (previousData) =>
      previousData || (startupApiModels.length > 0 ? startupApiModels : undefined),
  });

  /**
   * 函数 `invalidateAll`
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
  const invalidateAll = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["apikeys"] }),
      queryClient.invalidateQueries({ queryKey: ["apikey-models"] }),
      queryClient.invalidateQueries({ queryKey: ["apikey-usage-stats"] }),
      queryClient.invalidateQueries({ queryKey: ["apikey-usage-overview"] }),
      queryClient.invalidateQueries({ queryKey: ["startup-snapshot"] }),
    ]);
  };

  const createMutation = useMutation({
    mutationFn: (params: ApiKeyPayload) => accountClient.createApiKey(params),
    onSuccess: async () => {
      await invalidateAll();
      toast.success("密钥已创建");
    },
    onError: (error: unknown) => {
      toast.error(`创建失败: ${getAppErrorMessage(error)}`);
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => accountClient.deleteApiKey(id),
    onSuccess: async () => {
      await invalidateAll();
      toast.success("密钥已删除");
    },
    onError: (error: unknown) => {
      toast.error(`删除失败: ${getAppErrorMessage(error)}`);
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, params }: { id: string; params: ApiKeyPayload }) =>
      accountClient.updateApiKey(id, params),
    onSuccess: async (_result, variables) => {
      queryClient.setQueryData(["apikeys"], (current: unknown) =>
        Array.isArray(current)
          ? current.map((item) =>
              item && typeof item === "object" && "id" in item && item.id === variables.id
                ? {
                    ...item,
                    rotationStrategy:
                      variables.params.rotationStrategy ?? item.rotationStrategy,
                    aggregateApiId:
                      variables.params.aggregateApiId ?? item.aggregateApiId,
                  }
                : item,
            )
          : current,
      );
      await invalidateAll();
      toast.success("密钥配置已更新");
    },
    onError: (error: unknown) => {
      toast.error(`更新失败: ${getAppErrorMessage(error)}`);
    },
  });

  const toggleStatusMutation = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      enabled ? accountClient.enableApiKey(id) : accountClient.disableApiKey(id),
    onSuccess: async () => {
      await invalidateAll();
      toast.success("状态已更新");
    },
    onError: (error: unknown) => {
      toast.error(`更新状态失败: ${getAppErrorMessage(error)}`);
    },
  });

  const refreshApiKeysMutation = useMutation({
    mutationFn: async () => {
      const apiKeysResult = await apiKeysQuery.refetch();
      if (apiKeysResult.error) {
        throw apiKeysResult.error;
      }
      await Promise.all([
        queryClient.refetchQueries({ queryKey: ["apikey-usage-stats"], type: "active" }),
        queryClient.refetchQueries({ queryKey: ["apikey-usage-overview"], type: "active" }),
        queryClient.invalidateQueries({ queryKey: ["startup-snapshot"] }),
      ]);
    },
    onSuccess: () => {
      toast.success("平台密钥列表已刷新");
    },
    onError: (error: unknown) => {
      toast.error(`刷新失败: ${getAppErrorMessage(error)}`);
    },
  });

  const readSecretMutation = useMutation({
    mutationFn: (id: string) => accountClient.readApiKeySecret(id),
    onError: (error: unknown) => {
      toast.error(`读取密钥失败: ${getAppErrorMessage(error)}`);
    },
  });

  return {
    apiKeys: apiKeysQuery.data || [],
    models: modelsQuery.data || [],
    isLoading:
      isServiceReady &&
      !hasStartupApiKeySnapshot &&
      (!areApiKeyQueriesEnabled || apiKeysQuery.isLoading),
    isModelsLoading:
      isServiceReady &&
      !hasStartupApiModelSnapshot &&
      (!areApiKeyQueriesEnabled || modelsQuery.isLoading),
    isServiceReady,
    createApiKey: async (params: ApiKeyPayload) => {
      if (!ensureServiceReady("创建密钥")) return;
      await createMutation.mutateAsync(params);
    },
    deleteApiKey: (id: string) => {
      if (!ensureServiceReady("删除密钥")) return;
      deleteMutation.mutate(id);
    },
    updateApiKey: async (id: string, params: ApiKeyPayload) => {
      if (!ensureServiceReady("更新密钥")) return;
      await updateMutation.mutateAsync({ id, params });
    },
    toggleApiKeyStatus: ({ id, enabled }: { id: string; enabled: boolean }) => {
      if (!ensureServiceReady(enabled ? "启用密钥" : "禁用密钥")) return;
      toggleStatusMutation.mutate({ id, enabled });
    },
    refreshApiKeys: () => {
      if (!ensureServiceReady("刷新平台密钥列表")) return;
      refreshApiKeysMutation.mutate();
    },
    readApiKeySecret: async (id: string) => {
      if (!ensureServiceReady("读取密钥")) return "";
      return await readSecretMutation.mutateAsync(id);
    },
    isToggling: toggleStatusMutation.isPending,
    isRefreshingApiKeys: refreshApiKeysMutation.isPending,
    isReadingSecret: readSecretMutation.isPending,
  };
}
