"use client";

import { useMemo } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useRequestLogBodies } from "@/hooks/useRequestLogBodies";
import { getAppErrorMessage } from "@/lib/api/transport";
import { RequestLog } from "@/types";

type RequestLogBodyDialogProps = {
  log: RequestLog | null;
  serviceAddr: string;
  onOpenChange: (open: boolean) => void;
};

const BODY_PANEL_CLASS =
  "h-[60vh] overflow-auto rounded-lg border border-border/60 bg-muted/30 p-3 font-mono text-[11px] leading-relaxed break-all whitespace-pre-wrap";

function prettyPrintBody(raw: string): string {
  const text = raw.trim();
  if (!text) return "";
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

function ReadonlyBodyPanel({ content }: { content: string }) {
  return content ? (
    <pre className={BODY_PANEL_CLASS}>{content}</pre>
  ) : (
    <div className="flex h-[60vh] items-center justify-center rounded-lg border border-border/60 bg-muted/30 text-xs text-muted-foreground">
      暂无内容
    </div>
  );
}

export function RequestLogBodyDialog({
  log,
  serviceAddr,
  onOpenChange,
}: RequestLogBodyDialogProps) {
  const traceId = log?.traceId ?? "";
  const { data, isLoading, isError, error } = useRequestLogBodies(
    serviceAddr,
    traceId,
    Boolean(log)
  );
  const requestBody = useMemo(
    () => prettyPrintBody(data?.requestBody ?? ""),
    [data?.requestBody]
  );
  const responseBody = useMemo(
    () => prettyPrintBody(data?.responseBody ?? ""),
    [data?.responseBody]
  );

  return (
    <Dialog open={Boolean(log)} onOpenChange={onOpenChange}>
      <DialogContent className="md:max-w-[min(1024px,92vw)]">
        <DialogHeader>
          <DialogTitle>模型输入 / 输出</DialogTitle>
          <DialogDescription>
            {log
              ? `${log.model || "未知模型"} · ${log.requestPath || "-"}`
              : "按 trace 记录的单次调用内容"}
          </DialogDescription>
        </DialogHeader>
        {isLoading ? (
          <div className="flex flex-col gap-2">
            <Skeleton className="h-4 w-36" />
            <Skeleton className="h-48 w-full" />
          </div>
        ) : isError ? (
          <div className="rounded-lg border border-border/60 bg-muted/30 px-3 py-8 text-center text-xs text-muted-foreground">
            读取失败：{getAppErrorMessage(error, "请稍后重试")}
          </div>
        ) : (
          <Tabs defaultValue={log?.hasRequestBody ? "request" : "response"}>
            <TabsList>
              <TabsTrigger value="request" disabled={!log?.hasRequestBody}>
                输入
              </TabsTrigger>
              <TabsTrigger value="response" disabled={!log?.hasResponseBody}>
                输出
              </TabsTrigger>
            </TabsList>
            <TabsContent value="request">
              <ReadonlyBodyPanel content={requestBody} />
            </TabsContent>
            <TabsContent value="response">
              <ReadonlyBodyPanel content={responseBody} />
            </TabsContent>
          </Tabs>
        )}
      </DialogContent>
    </Dialog>
  );
}
