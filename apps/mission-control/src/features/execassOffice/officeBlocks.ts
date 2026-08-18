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
  | "next"
  | "room-shortcut";

export interface OfficeBlockDef extends BlockDef {
  rendererKey: OfficeBlockRendererKey;
  /** For room-shortcut blocks: the stable room id the shortcut opens. */
  roomId?: string;
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
  // Pinned-from-the-Trenches shortcuts: hidden until the boss pins them.
  // A shortcut deep-links to its room by stable id and never copies data.
  {
    id: "boards",
    rendererKey: "room-shortcut",
    roomId: "boards",
    title: "Boards",
    defaultSize: "s",
    defaultVisible: false,
  },
  {
    id: "calendar",
    rendererKey: "room-shortcut",
    roomId: "calendar",
    title: "Calendar",
    defaultSize: "s",
    defaultVisible: false,
  },
  // The Plan room's registry-declared block id is "strategy" (its route),
  // so the shortcut carries that id while opening the "plan" room.
  {
    id: "strategy",
    rendererKey: "room-shortcut",
    roomId: "plan",
    title: "Plan",
    defaultSize: "s",
    defaultVisible: false,
  },
  {
    id: "staff",
    rendererKey: "room-shortcut",
    roomId: "staff",
    title: "Staff Directory",
    defaultSize: "s",
    defaultVisible: false,
  },
  {
    id: "history",
    rendererKey: "room-shortcut",
    roomId: "history",
    title: "History & Receipts",
    defaultSize: "s",
    defaultVisible: false,
  },
  // The Basement's first shortcut opens the Connectors room by stable id;
  // the Setup room shares the route but gets its own block in a later slice.
  {
    id: "connectors",
    rendererKey: "room-shortcut",
    roomId: "connectors",
    title: "Connectors",
    defaultSize: "s",
    defaultVisible: false,
  },
  {
    id: "models",
    rendererKey: "room-shortcut",
    roomId: "models",
    title: "Models & Providers",
    defaultSize: "s",
    defaultVisible: false,
  },
  {
    id: "events",
    rendererKey: "room-shortcut",
    roomId: "events",
    title: "Event stream",
    defaultSize: "s",
    defaultVisible: false,
  },
  {
    id: "breakers",
    rendererKey: "room-shortcut",
    roomId: "breakers",
    title: "Breakers & Scheduler",
    defaultSize: "s",
    defaultVisible: false,
  },
  {
    id: "directory",
    rendererKey: "room-shortcut",
    roomId: "directory",
    title: "Directory / Front Desk",
    defaultSize: "s",
    defaultVisible: false,
  },
  {
    id: "memory",
    rendererKey: "room-shortcut",
    roomId: "memory",
    title: "Memory plant",
    defaultSize: "s",
    defaultVisible: false,
  },
  {
    id: "locks",
    rendererKey: "room-shortcut",
    roomId: "locks",
    title: "File locks",
    defaultSize: "s",
    defaultVisible: false,
  },
  {
    id: "setup",
    rendererKey: "room-shortcut",
    roomId: "setup",
    title: "Setup",
    defaultSize: "s",
    defaultVisible: false,
  },
  {
    id: "policy",
    rendererKey: "room-shortcut",
    roomId: "policy",
    title: "Policy",
    defaultSize: "s",
    defaultVisible: false,
  },
];
