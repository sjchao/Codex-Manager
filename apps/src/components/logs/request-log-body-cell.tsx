"use client";

import { Button } from "@/components/ui/button";

type RequestLogBodyCellProps = {
  hasRequestBody: boolean;
  hasResponseBody: boolean;
  onOpen: () => void;
};

export function RequestLogBodyCell({
  hasRequestBody,
  hasResponseBody,
  onOpen,
}: RequestLogBodyCellProps) {
  if (!hasRequestBody && !hasResponseBody) {
    return <span className="text-muted-foreground">-</span>;
  }

  const label = [hasRequestBody ? "输入" : "", hasResponseBody ? "输出" : ""]
    .filter(Boolean)
    .join("·");

  return (
    <Button
      type="button"
      variant="outline"
      size="xs"
      title="查看模型输入 / 输出"
      onClick={onOpen}
    >
      查看{label}
    </Button>
  );
}
