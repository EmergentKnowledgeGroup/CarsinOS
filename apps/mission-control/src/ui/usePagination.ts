/** Hook for paginating an array */
export function usePagination<T>(items: T[], pageSize: number) {
  const safePageSize = Number.isFinite(pageSize) && pageSize > 0 ? Math.floor(pageSize) : 1;
  const totalPages = Math.max(1, Math.ceil(items.length / safePageSize));
  const clampPage = (page: number) => {
    if (!Number.isFinite(page)) {
      return 1;
    }
    return Math.min(totalPages, Math.max(1, Math.floor(page)));
  };

  return {
    totalPages,
    clampPage,
    getPage: (page: number) => {
      const normalizedPage = clampPage(page);
      const start = (normalizedPage - 1) * safePageSize;
      return items.slice(start, start + safePageSize);
    },
  };
}
