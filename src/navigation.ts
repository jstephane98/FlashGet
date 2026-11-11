import { CalendarDays, CircleCheck, Clock, List, Plus, Settings, type LucideIcon } from "lucide-react";
import type { MsgKey } from "./i18n";

export type PageId = "new" | "downloads" | "completed" | "queue" | "scheduler" | "settings";

export interface NavItem {
  id: PageId;
  labelKey: MsgKey;
  icon: LucideIcon;
  ready: boolean;
  bottom?: boolean;
}

export const NAV_ITEMS: NavItem[] = [
  { id: "new", labelKey: "nav.new", icon: Plus, ready: true },
  { id: "downloads", labelKey: "nav.downloads", icon: List, ready: true },
  { id: "completed", labelKey: "nav.completed", icon: CircleCheck, ready: true },
  { id: "queue", labelKey: "nav.queue", icon: Clock, ready: true },
  { id: "scheduler", labelKey: "nav.scheduler", icon: CalendarDays, ready: true },
  { id: "settings", labelKey: "nav.settings", icon: Settings, ready: true, bottom: true },
];
