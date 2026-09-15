"use client";

import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Plus, RefreshCw, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { ConfirmDialog } from "@/components/modals/confirm-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { accountClient } from "@/lib/api/account-client";
import { formatTsFromSeconds } from "@/lib/utils/usage";
import { Sub2ApiAccount } from "@/types";

const SUB2API_ACCOUNTS_QUERY_KEY = ["sub2api-accounts"];

const formatMoney = (value: number | null) =>
  typeof value === "number" && Number.isFinite(value)
    ? `$${value.toFixed(6)}`
    : "-";

const isToday = (value: number | null) => {
  if (!value) return false;
  const current = new Date();
  const timestamp = new Date(value * 1000);
  return (
    current.getFullYear() === timestamp.getFullYear() &&
    current.getMonth() === timestamp.getMonth() &&
    current.getDate() === timestamp.getDate()
  );
};

export function Sub2ApiAccountsPanel({ enabled }: { enabled: boolean }) {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState<Sub2ApiAccount | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const [baseUrl, setBaseUrl] = useState("");
  const [authToken, setAuthToken] = useState("");
  const [refreshToken, setRefreshToken] = useState("");
  const [expiresAt, setExpiresAt] = useState("");
  const [syncingIds, setSyncingIds] = useState<string[]>([]);
  const syncingRefCount = useRef<Record<string, number>>({});
  const initialSyncStartedRef = useRef(false);

  const { data: accounts = [], isLoading } = useQuery({
    queryKey: SUB2API_ACCOUNTS_QUERY_KEY,
    queryFn: () => accountClient.listSub2Api(),
    enabled,
    refetchInterval: 60_000,
  });

  useEffect(() => {
    if (!enabled || initialSyncStartedRef.current) return;
    initialSyncStartedRef.current = true;
    void accountClient
      .syncAllSub2Api()
      .then(() =>
        queryClient.invalidateQueries({
          queryKey: SUB2API_ACCOUNTS_QUERY_KEY,
        }),
      )
      .catch(() => undefined);
  }, [enabled, queryClient]);

  const save = useMutation({
    mutationFn: async () => {
      const expires = expiresAt.trim() ? Number(expiresAt.trim()) : null;
      if (expiresAt.trim() && !Number.isFinite(expires)) {
        throw new Error("token_expires_at 必须是 Unix 时间戳");
      }
      if (editing) {
        return accountClient.updateSub2Api(editing.id, {
          baseUrl,
          authToken: authToken.trim() || null,
          refreshToken: refreshToken.trim() || null,
          tokenExpiresAt: expires,
        });
      }
      if (!authToken.trim()) {
        throw new Error("新增时必须填写 auth_token");
      }
      return accountClient.createSub2Api({
        baseUrl,
        authToken: authToken.trim(),
        refreshToken: refreshToken.trim() || null,
        tokenExpiresAt: expires,
      });
    },
    onSuccess: async () => {
      toast.success("Sub2API 已保存");
      setOpen(false);
      await queryClient.invalidateQueries({
        queryKey: SUB2API_ACCOUNTS_QUERY_KEY,
      });
    },
    onError: (error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    },
  });

  const sync = useMutation({
    mutationFn: (id: string) => accountClient.syncSub2Api(id),
    onSuccess: async () => {
      toast.success("同步完成");
      await queryClient.invalidateQueries({
        queryKey: SUB2API_ACCOUNTS_QUERY_KEY,
      });
    },
    onError: (error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    },
  });

  const markSyncing = (id: string) => {
    syncingRefCount.current[id] = (syncingRefCount.current[id] ?? 0) + 1;
    setSyncingIds((prev) => (prev.includes(id) ? prev : [...prev, id]));
  };

  const unmarkSyncing = (id: string) => {
    const next = (syncingRefCount.current[id] ?? 0) - 1;
    if (next > 0) {
      syncingRefCount.current[id] = next;
      return;
    }
    delete syncingRefCount.current[id];
    setSyncingIds((prev) => prev.filter((item) => item !== id));
  };

  const syncAll = useMutation({
    mutationFn: async (ids: string[]) => {
      ids.forEach(markSyncing);
      try {
        await Promise.all(
          ids.map((id) =>
            sync
              .mutateAsync(id)
              .catch(() => undefined)
              .finally(() => unmarkSyncing(id))
          )
        );
      } finally {
        syncingRefCount.current = {};
        setSyncingIds([]);
      }
    },
  });

  const syncAccount = (id: string) => {
    if (syncingRefCount.current[id]) return;
    markSyncing(id);
    // 用 promise 兜底清理：MutationObserver 换绑会让先前 mutation 的 onSettled 不再触发
    void sync.mutateAsync(id).finally(() => unmarkSyncing(id));
  };

  const syncAllAccounts = () =>
    syncAll.mutate(accounts.map((account) => account.id));

  const remove = useMutation({
    mutationFn: (id: string) => accountClient.deleteSub2Api(id),
    onSuccess: async () => {
      toast.success("已删除");
      await queryClient.invalidateQueries({
        queryKey: SUB2API_ACCOUNTS_QUERY_KEY,
      });
    },
    onError: (error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    },
  });

  const openCreate = () => {
    setEditing(null);
    setBaseUrl("");
    setAuthToken("");
    setRefreshToken("");
    setExpiresAt("");
    setOpen(true);
  };

  const openEdit = (account: Sub2ApiAccount) => {
    setEditing(account);
    setBaseUrl(account.baseUrl);
    setAuthToken("");
    setRefreshToken("");
    setExpiresAt("");
    setOpen(true);
  };

  return (
    <div className="space-y-4">
      <Card className="glass-card border-none shadow-xl backdrop-blur-md">
        <CardContent className="px-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="text-xs text-muted-foreground">
              共 {accounts.length} 个账户
            </div>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                className="h-10 gap-2"
                onClick={syncAllAccounts}
                disabled={
                  !enabled || !accounts.length || syncAll.isPending || !!syncingIds.length
                }
              >
                <RefreshCw
                  className={
                    syncAll.isPending ? "h-4 w-4 animate-spin" : "h-4 w-4"
                  }
                />
                一键刷新
              </Button>
              <Button
                className="h-10 gap-2 shadow-lg shadow-primary/20"
                onClick={openCreate}
                disabled={!enabled}
              >
                <Plus className="h-4 w-4" /> 新增 Sub2API
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card className="glass-card overflow-hidden border-none py-0 shadow-xl backdrop-blur-md">
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>账户</TableHead>
                <TableHead>地址</TableHead>
                <TableHead>余额</TableHead>
                <TableHead>当日消费</TableHead>
                <TableHead>请求数</TableHead>
                <TableHead>Token 到期</TableHead>
                <TableHead>同步状态</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {isLoading ? (
                <TableRow>
                  <TableCell
                    colSpan={8}
                    className="py-10 text-center text-muted-foreground"
                  >
                    加载中...
                  </TableCell>
                </TableRow>
              ) : (
                accounts.map((account) => {
                  const currentDay = isToday(account.lastSyncAt);
                  const isSyncing = syncingIds.includes(account.id);
                  return (
                    <TableRow key={account.id}>
                      <TableCell>
                        <div className="font-medium">
                          {account.accountName || "未获取账户名"}
                        </div>
                        <div className="text-xs text-muted-foreground">
                          {account.accountEmail || "-"}
                        </div>
                      </TableCell>
                      <TableCell className="max-w-[240px] truncate font-mono text-xs">
                        {account.baseUrl}
                      </TableCell>
                      <TableCell>{formatMoney(account.balance)}</TableCell>
                      <TableCell>
                        {currentDay ? formatMoney(account.todayActualCost) : "-"}
                      </TableCell>
                      <TableCell>
                        {currentDay ? (account.todayRequestCount ?? "-") : "-"}
                      </TableCell>
                      <TableCell>
                        {account.tokenExpiresAt ? (
                          <span
                            className={
                              account.tokenExpiresAt * 1000 <= Date.now()
                                ? "text-destructive"
                                : undefined
                            }
                          >
                            {formatTsFromSeconds(account.tokenExpiresAt)}
                            {account.tokenExpiresAt * 1000 <= Date.now()
                              ? "（已过期）"
                              : ""}
                          </span>
                        ) : (
                          <span className="text-xs text-muted-foreground">
                            未知
                          </span>
                        )}
                      </TableCell>
                      <TableCell>
                        {account.lastSyncStatus === "success" ? (
                          <Badge variant="secondary">成功</Badge>
                        ) : account.lastSyncStatus === "failed" ? (
                          <Badge variant="destructive">失败</Badge>
                        ) : (
                          <Badge variant="outline">未同步</Badge>
                        )}
                        <div className="text-xs text-muted-foreground">
                          {formatTsFromSeconds(account.lastSyncAt, "-")}
                        </div>
                      </TableCell>
                      <TableCell className="text-right">
                        <div className="flex justify-end gap-1">
                          <Button
                            variant="ghost"
                            size="icon"
                            title="同步"
                            disabled={isSyncing || syncAll.isPending}
                            onClick={() => syncAccount(account.id)}
                          >
                            <RefreshCw
                              className={
                                isSyncing
                                  ? "h-4 w-4 animate-spin"
                                  : "h-4 w-4"
                              }
                            />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon"
                            title="编辑"
                            disabled={!enabled}
                            onClick={() => openEdit(account)}
                          >
                            <Pencil className="h-4 w-4" />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon"
                            title="删除"
                            disabled={!enabled || remove.isPending}
                            onClick={() => setDeleteId(account.id)}
                          >
                            <Trash2 className="h-4 w-4" />
                          </Button>
                        </div>
                      </TableCell>
                    </TableRow>
                  );
                })
              )}
              {!isLoading && accounts.length === 0 ? (
                <TableRow>
                  <TableCell
                    colSpan={8}
                    className="py-10 text-center text-muted-foreground"
                  >
                    暂无 Sub2API 账户
                  </TableCell>
                </TableRow>
              ) : null}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{editing ? "编辑 Sub2API" : "新增 Sub2API"}</DialogTitle>
          </DialogHeader>
          <div className="grid gap-4 py-2">
            <div className="grid gap-2">
              <Label htmlFor="sub2api-url">Base URL</Label>
              <Input
                id="sub2api-url"
                value={baseUrl}
                onChange={(event) => setBaseUrl(event.target.value)}
                placeholder="https://example.com"
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="sub2api-auth">auth_token</Label>
              <Input
                id="sub2api-auth"
                type="password"
                value={authToken}
                onChange={(event) => setAuthToken(event.target.value)}
                placeholder={editing ? "留空保持原值" : "面板 JWT"}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="sub2api-refresh">refresh_token</Label>
              <Input
                id="sub2api-refresh"
                type="password"
                value={refreshToken}
                onChange={(event) => setRefreshToken(event.target.value)}
                placeholder="可选"
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="sub2api-expires">token_expires_at</Label>
              <Input
                id="sub2api-expires"
                inputMode="numeric"
                value={expiresAt}
                onChange={(event) => setExpiresAt(event.target.value)}
                placeholder="Unix 秒时间戳，可选"
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>
              取消
            </Button>
            <Button onClick={() => save.mutate()} disabled={save.isPending}>
              {save.isPending ? "同步中..." : "保存并同步"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <ConfirmDialog
        open={Boolean(deleteId)}
        onOpenChange={(nextOpen) => !nextOpen && setDeleteId(null)}
        title="删除 Sub2API 账户"
        description="删除后将不再同步该面板的余额与用量，是否确认删除？"
        confirmText="删除"
        cancelText="取消"
        onConfirm={() => {
          if (!deleteId) return;
          remove.mutate(deleteId);
          setDeleteId(null);
        }}
      />
    </div>
  );
}
