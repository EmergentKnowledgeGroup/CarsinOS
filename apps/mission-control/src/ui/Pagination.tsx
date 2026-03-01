import clsx from "clsx";

interface PaginationProps {
  currentPage: number;
  totalPages: number;
  onPageChange: (page: number) => void;
  className?: string;
}

/**
 * Simple numbered pagination. Shows prev/next + page numbers.
 * Replaces scroll bars — Law 1 compliance.
 */
export function Pagination({ currentPage, totalPages, onPageChange, className }: PaginationProps) {
  if (totalPages <= 1) return null;
  const safeCurrentPage = Math.min(Math.max(1, currentPage), totalPages);

  return (
    <nav className={clsx("mc-pagination", className)}>
      <button
        type="button"
        className="mc-pagination-btn"
        disabled={safeCurrentPage <= 1}
        onClick={() => onPageChange(safeCurrentPage - 1)}
      >
        Prev
      </button>
      <span className="mc-pagination-info">
        {safeCurrentPage} / {totalPages}
      </span>
      <button
        type="button"
        className="mc-pagination-btn"
        disabled={safeCurrentPage >= totalPages}
        onClick={() => onPageChange(safeCurrentPage + 1)}
      >
        Next
      </button>
    </nav>
  );
}
