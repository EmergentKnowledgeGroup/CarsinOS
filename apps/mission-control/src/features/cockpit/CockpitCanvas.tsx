import { useMemo, type ReactNode } from "react";
import {
  WIDGET_SIZE_CONSTRAINTS,
  type CockpitWidgetLayoutV2,
} from "./cockpitLayout";

// react-grid-layout types use `export =` which doesn't play well with
// verbatimModuleSyntax. We import at runtime and cast.
import RGLModule from "react-grid-layout";

import "react-grid-layout/css/styles.css";
import "react-resizable/css/styles.css";

// The package exports Responsive and WidthProvider as named runtime exports
// even though the type bundle declares them via namespace.
const RGL = RGLModule as unknown as {
  Responsive: React.ComponentType<Record<string, unknown>>;
  WidthProvider: <P extends object>(
    component: React.ComponentType<P>,
  ) => React.ComponentType<P & { measureBeforeMount?: boolean }>;
};

const ResponsiveGrid = RGL.WidthProvider(RGL.Responsive);

interface LayoutItem {
  i: string;
  x: number;
  y: number;
  w: number;
  h: number;
  minW?: number;
  minH?: number;
}

interface CockpitCanvasProps {
  widgets: CockpitWidgetLayoutV2[];
  editMode: boolean;
  onLayoutChange: (layout: LayoutItem[]) => void;
  children: ReactNode;
}

export function CockpitCanvas({
  widgets,
  editMode,
  onLayoutChange,
  children,
}: CockpitCanvasProps) {
  const layouts = useMemo(() => {
    const lg: LayoutItem[] = widgets.map((w) => {
      const kind = w.widget;
      const constraints =
        kind === "custom"
          ? { minW: 3, minH: 2 }
          : WIDGET_SIZE_CONSTRAINTS[kind];

      return {
        i: w.instance_id,
        x: w.position.x,
        y: w.position.y,
        w: w.position.w,
        h: w.position.h,
        minW: constraints.minW,
        minH: constraints.minH,
      };
    });
    return { lg };
  }, [widgets]);

  return (
    <ResponsiveGrid
      className="mc-rgl-canvas"
      layouts={layouts}
      breakpoints={{ lg: 996, md: 768, sm: 480, xs: 0 }}
      cols={{ lg: 12, md: 8, sm: 4, xs: 2 }}
      rowHeight={60}
      margin={[12, 12]}
      containerPadding={[12, 12]}
      isDraggable={editMode}
      isResizable={editMode}
      draggableHandle=".mc-widget-drag-handle"
      compactType="vertical"
      onLayoutChange={(layout: LayoutItem[]) => onLayoutChange(layout)}
    >
      {children}
    </ResponsiveGrid>
  );
}
