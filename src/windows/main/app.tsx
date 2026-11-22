import type { MainRoute } from "@/lib/ipc";

import { useCallback, useEffect, useState } from "react";

import { cn } from "cn";

import { Icon } from "@/components/ui/icon";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
} from "@/components/ui/sidebar";
import { useIpcEvent } from "@/hooks/use-ipc-event";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import { useSettings } from "@/hooks/use-settings";
import { listModels } from "@/lib/ipc";

import { hashFor, useLocation } from "./route";
import { ROUTES, TOP_ROUTES } from "./routes";

function NavItem({ route, active }: { active: boolean; route: MainRoute }) {
  const { icon, label } = ROUTES[route];
  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        aria-current={active ? "page" : undefined}
        isActive={active}
        render={<a href={hashFor(route)} />}
      >
        <Icon aria-hidden="true" icon={icon} />
        <span>{label}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}

/** The voice model in use, or a nudge to get one. */
function ModelStatus() {
  const { settings } = useSettings();
  const models = useIpcQuery(listModels, ["models:changed"]);
  if (!(settings && models.data)) {
    return <div className="h-11" />;
  }
  const model = models.data.find(
    (m) => m.id === settings.sttModel && m.status.state === "downloaded"
  );
  return (
    <a
      className="flex flex-col gap-0.5 rounded-lg px-2 py-1.5 outline-none transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-foreground motion-reduce:transition-none"
      href={hashFor("models")}
    >
      <span className="flex items-center gap-2 text-foreground text-xs">
        <span
          aria-hidden="true"
          className={cn(
            "size-1.5 rounded-full",
            model ? "bg-success" : "bg-warning"
          )}
        />
        {model ? "Ready" : "No voice model"}
      </span>
      <span className="truncate ps-3.5 text-foreground-muted text-xs">
        {model ? model.label : "Choose one in Models"}
      </span>
    </a>
  );
}

const isSettingsShortcut = (event: KeyboardEvent) =>
  event.key === "," &&
  (event.metaKey || event.ctrlKey) &&
  !event.altKey &&
  !event.shiftKey;

export function App() {
  const [location, navigate] = useLocation();
  useIpcEvent("window:navigate", (payload) => navigate(payload.route));

  const { component: Page, label } = ROUTES[location.route];
  const [scrolled, setScrolled] = useState(false);
  const onScroll = useCallback(
    (event: React.UIEvent<HTMLElement>) =>
      setScrolled(event.currentTarget.scrollTop > 0),
    []
  );

  useEffect(() => {
    document.title = `${label} — Tiny Wispr`;
  }, [label]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (isSettingsShortcut(event)) {
        event.preventDefault();
        navigate("settings");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [navigate]);

  return (
    <SidebarProvider className="h-svh min-h-0">
      <Sidebar
        className="h-svh border-e bg-background-muted"
        collapsible="none"
      >
        {/* Room for the macOS traffic lights; also the window's drag handle. */}
        <SidebarHeader className="h-12 shrink-0" data-tauri-drag-region />
        <div className="px-4 pb-4" data-tauri-drag-region>
          <p className="font-redaction text-foreground text-xl">Tiny Wispr</p>
        </div>
        <SidebarContent>
          <SidebarGroup>
            <nav aria-label="Sections">
              <SidebarMenu>
                {TOP_ROUTES.map((r) => (
                  <NavItem active={r === location.route} key={r} route={r} />
                ))}
              </SidebarMenu>
            </nav>
          </SidebarGroup>
        </SidebarContent>
        <SidebarFooter className="gap-2">
          <ModelStatus />
          <SidebarMenu>
            <NavItem active={location.route === "settings"} route="settings" />
          </SidebarMenu>
        </SidebarFooter>
      </Sidebar>
      <SidebarInset
        className="min-h-0 overflow-y-auto overscroll-none [scrollbar-gutter:stable]"
        onScroll={onScroll}
      >
        {/* Content scrolls under this bar rather than being cut off by it. */}
        <div
          className={cn(
            "sticky top-0 z-10 h-12 shrink-0 border-b bg-background/80 backdrop-blur-md transition-colors",
            scrolled ? "border-foreground/10" : "border-transparent"
          )}
          data-tauri-drag-region
        />
        <Page section={location.section} />
      </SidebarInset>
    </SidebarProvider>
  );
}
