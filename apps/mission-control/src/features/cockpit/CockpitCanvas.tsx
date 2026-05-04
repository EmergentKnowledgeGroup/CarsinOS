import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  COCKPIT_GRID_COLS,
  WIDGET_SIZE_CONSTRAINTS,
  type CockpitWidgetLayoutV2,
} from "./cockpitLayout";

// react-grid-layout types use `export =` which doesn't play well with
// verbatimModuleSyntax. We import at runtime and cast.
import RGLModule from "react-grid-layout";

import "react-grid-layout/css/styles.css";
import "react-resizable/css/styles.css";

interface LayoutItem {
  i: string;
  x: number;
  y: number;
  w: number;
  h: number;
  minW?: number;
  minH?: number;
}

interface GridLayoutProps {
  className: string;
  layout: LayoutItem[];
  width: number;
  cols: number;
  rowHeight: number;
  margin: [number, number];
  containerPadding: [number, number];
  isDraggable: boolean;
  isResizable: boolean;
  isBounded: boolean;
  preventCollision: boolean;
  useCSSTransforms: boolean;
  resizeHandles: string[];
  draggableCancel: string;
  compactType: null;
  onDragStop: (layout: LayoutItem[]) => void;
  onResizeStop: (layout: LayoutItem[]) => void;
  children: ReactNode;
}

const GridLayout = RGLModule as unknown as React.ComponentType<GridLayoutProps>;

interface CockpitCanvasProps {
  widgets: CockpitWidgetLayoutV2[];
  editMode: boolean;
  isActive: boolean;
  onLayoutChange: (layout: LayoutItem[]) => void;
  children: ReactNode;
}

export function CockpitCanvas({
  widgets,
  editMode,
  isActive,
  onLayoutChange,
  children,
}: CockpitCanvasProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [containerWidth, setContainerWidth] = useState(1200);

  const measureContainer = useCallback(() => {
    const element = containerRef.current;
    if (!element) {
      return;
    }
    const nextWidth = Math.max(640, Math.round(element.clientWidth));
    setContainerWidth((prev) => (prev === nextWidth ? prev : nextWidth));
  }, []);

  useEffect(() => {
    const element = containerRef.current;
    if (!element) {
      return;
    }
    measureContainer();
    const observer = new ResizeObserver(() => {
      measureContainer();
    });
    observer.observe(element);
    window.addEventListener("resize", measureContainer);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", measureContainer);
    };
  }, [measureContainer]);

  useEffect(() => {
    measureContainer();
  }, [editMode, isActive, measureContainer]);

  const layout = useMemo(() => {
    return widgets.map((w) => {
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
  }, [widgets]);

  return (
    <div ref={containerRef} className="mc-rgl-shell">
      <GridLayout
        className="mc-rgl-canvas"
        layout={layout}
        width={containerWidth}
        cols={COCKPIT_GRID_COLS}
        rowHeight={48}
        margin={[12, 12]}
        containerPadding={[12, 12]}
        isDraggable={editMode}
        isResizable={editMode}
        isBounded
        preventCollision
        useCSSTransforms={false}
        resizeHandles={["e", "s", "se"]}
        draggableCancel=".mc-widget-nudge-controls, .mc-widget-remove-btn, .mc-widget-nudge-btn, .mc-cockpit-edit-toolbar, .mc-edit-toolbar-btn, .mc-edit-toolbar-dropdown, input, textarea, select, button, a"
        compactType={null}
        onDragStop={(currentLayout: LayoutItem[]) => {
          if (!editMode) {
            return;
          }
          onLayoutChange(currentLayout);
        }}
        onResizeStop={(currentLayout: LayoutItem[]) => {
          if (!editMode) {
            return;
          }
          onLayoutChange(currentLayout);
        }}
      >
        {children}
      </GridLayout>
    </div>
  );
}
