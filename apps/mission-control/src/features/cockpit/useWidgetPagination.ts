import { useState, useEffect, useCallback, useMemo, type RefObject } from "react";

interface WidgetPaginationResult {
  page: number;
  setPage: (page: number) => void;
  pageSize: number;
  totalPages: number;
  startIndex: number;
  endIndex: number;
}

export function useWidgetPagination(
  totalItems: number,
  containerRef: RefObject<HTMLElement | null>,
  itemHeight: number,
): WidgetPaginationResult {
  const [rawPage, setRawPage] = useState(0);
  const [pageSize, setPageSize] = useState(5);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const height = entry.contentRect.height;
        const computed = Math.max(1, Math.floor(height / itemHeight));
        setPageSize(computed);
      }
    });

    observer.observe(el);
    return () => observer.disconnect();
  }, [containerRef, itemHeight]);

  const totalPages = Math.max(1, Math.ceil(totalItems / pageSize));
  const page = Math.min(rawPage, totalPages - 1);

  const startIndex = page * pageSize;
  const endIndex = Math.min(startIndex + pageSize, totalItems);

  const handleSetPage = useCallback(
    (next: number) => {
      setRawPage(Math.max(0, Math.min(totalPages - 1, next)));
    },
    [totalPages],
  );

  return useMemo(
    () => ({
      page,
      setPage: handleSetPage,
      pageSize,
      totalPages,
      startIndex,
      endIndex,
    }),
    [page, handleSetPage, pageSize, totalPages, startIndex, endIndex],
  );
}
