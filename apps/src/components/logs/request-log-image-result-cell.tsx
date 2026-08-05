"use client";

import { Skeleton } from "@/components/ui/skeleton";
import { useRequestLogImages } from "@/hooks/useRequestLogImages";
import {
  RequestLogImageData,
  RequestLogImageResult,
} from "@/types";

type RequestLogImageResultCellProps = {
  traceId: string;
  imageResults: RequestLogImageResult[];
  serviceAddr: string;
  onPreview: (image: RequestLogImageData) => void;
};

export function RequestLogImageResultCell({
  traceId,
  imageResults,
  serviceAddr,
  onPreview,
}: RequestLogImageResultCellProps) {
  const { data: imageData = [], isError, isLoading } = useRequestLogImages(
    serviceAddr,
    traceId,
    imageResults.length > 0
  );

  if (!traceId || imageResults.length === 0 || isError) {
    return <span className="text-muted-foreground">-</span>;
  }

  if (isLoading) {
    return (
      <div className="flex flex-nowrap gap-2 overflow-hidden">
        {imageResults.map((image) => (
          <Skeleton key={image.storageKey} className="h-12 w-16 shrink-0" />
        ))}
      </div>
    );
  }

  const imageByStorageKey = new Map(
    imageData.map((image) => [image.storageKey, image])
  );
  const availableImages = imageResults
    .map((imageResult) => imageByStorageKey.get(imageResult.storageKey))
    .filter((image): image is RequestLogImageData => Boolean(image));

  if (availableImages.length === 0) {
    return <span className="text-muted-foreground">-</span>;
  }

  return (
    <div className="flex flex-nowrap gap-2 overflow-x-auto pb-1">
      {availableImages.map((image, index) => (
        <button
          key={image.storageKey}
          type="button"
          className="h-12 w-16 shrink-0 overflow-hidden rounded-md border border-border/70 bg-muted/30 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
          aria-label={`查看第 ${index + 1} 张生成图片`}
          onClick={() => onPreview(image)}
        >
          <img
            src={image.dataUrl}
            alt={`第 ${index + 1} 张生成图片`}
            className="h-full w-full object-cover"
          />
        </button>
      ))}
    </div>
  );
}
