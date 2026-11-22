import type { IconSvgElement } from "@hugeicons/react";
import type { MainRoute } from "@/lib/ipc";
import type { SettingsSection } from "./route";

import {
  AiChipIcon,
  Book02Icon,
  Home01Icon,
  PaintBrush01Icon,
  Settings01Icon,
  TextIcon,
} from "@hugeicons/core-free-icons";

import { DictionaryPage } from "./pages/dictionary";
import { HomePage } from "./pages/home";
import { ModelsPage } from "./pages/models";
import { SettingsPage } from "./pages/settings";
import { SnippetsPage } from "./pages/snippets";
import { StylePage } from "./pages/style";

export interface RouteDefinition {
  component: (props: { section: SettingsSection | null }) => React.ReactNode;
  icon: IconSvgElement;
  label: string;
}

export const ROUTES: Record<MainRoute, RouteDefinition> = {
  dictionary: {
    component: DictionaryPage,
    icon: Book02Icon,
    label: "Dictionary",
  },
  home: { component: HomePage, icon: Home01Icon, label: "Home" },
  models: { component: ModelsPage, icon: AiChipIcon, label: "Models" },
  settings: {
    component: SettingsPage,
    icon: Settings01Icon,
    label: "Settings",
  },
  snippets: { component: SnippetsPage, icon: TextIcon, label: "Snippets" },
  style: { component: StylePage, icon: PaintBrush01Icon, label: "Style" },
};

/** Sidebar order. Settings sits at the bottom, apart from the rest. */
export const TOP_ROUTES: MainRoute[] = [
  "home",
  "dictionary",
  "snippets",
  "style",
  "models",
];
