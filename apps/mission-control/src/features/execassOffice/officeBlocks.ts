/**
 * The Office block registry: every canvas section is a registered block so
 * layout stays config, "Pin to Office" is a registry entry, and future
 * floors extend this list instead of hardcoding new sections.
 */

import type { BlockDef } from "../../glass/blocks";

export type OfficeBlockRendererKey =
  | "needs-you"
  | "in-motion"
  | "done"
  | "next";

export interface OfficeBlockDef extends BlockDef {
  rendererKey: OfficeBlockRendererKey;
}

export const OFFICE_BLOCK_REGISTRY: readonly OfficeBlockDef[] = [
  {
    id: "needs-you",
    rendererKey: "needs-you",
    title: "Needs you",
    defaultSize: "l",
    defaultVisible: true,
  },
  {
    id: "in-motion",
    rendererKey: "in-motion",
    title: "In motion",
    defaultSize: "m",
    defaultVisible: true,
  },
  {
    id: "done",
    rendererKey: "done",
    title: "Done since you checked",
    defaultSize: "m",
    defaultVisible: true,
  },
  {
    id: "next",
    rendererKey: "next",
    title: "Next",
    defaultSize: "s",
    defaultVisible: true,
  },
];
