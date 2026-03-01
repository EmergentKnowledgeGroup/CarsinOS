import clsx from "clsx";

interface Tab {
  id: string;
  label: string;
  /** Optional count badge */
  count?: number;
}

interface TabsProps {
  tabs: Tab[];
  activeTab: string;
  onTabChange: (tabId: string) => void;
  className?: string;
}

/**
 * Horizontal sub-tab bar for within-page navigation.
 * Used inside feature pages (Calendar, Focus, Mail, etc.) — not the main nav rail.
 */
export function Tabs({ tabs, activeTab, onTabChange, className }: TabsProps) {
  return (
    <div className={clsx("mc-sub-tabs", className)}>
      {tabs.map((tab) => (
        <button
          key={tab.id}
          type="button"
          className={clsx("mc-sub-tab", activeTab === tab.id && "mc-sub-tab-active")}
          onClick={() => onTabChange(tab.id)}
        >
          {tab.label}
          {tab.count !== undefined && tab.count > 0 ? (
            <span className="mc-sub-tab-count">{tab.count}</span>
          ) : null}
        </button>
      ))}
    </div>
  );
}
