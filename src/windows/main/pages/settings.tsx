import type { SettingsSection } from "../route";

import {
  SegmentedControl,
  SegmentedControlItem,
} from "@/components/ui/segmented-control";

import { Page } from "../page";
import { hashFor, SETTINGS_SECTIONS } from "../route";
import { AboutSettings } from "../settings/about";
import { GeneralSettings } from "../settings/general";
import { PrivacySettings } from "../settings/privacy";
import { SystemSettings } from "../settings/system";

const SECTIONS: Record<
  SettingsSection,
  { component: () => React.ReactNode; label: string }
> = {
  about: { component: AboutSettings, label: "About" },
  general: { component: GeneralSettings, label: "General" },
  privacy: { component: PrivacySettings, label: "Privacy" },
  system: { component: SystemSettings, label: "System" },
};

export function SettingsPage({ section }: { section: SettingsSection | null }) {
  const current = section ?? "general";
  const Section = SECTIONS[current].component;
  return (
    <Page title="Settings">
      <SegmentedControl
        aria-label="Settings sections"
        className="-mt-4 self-start"
        render={<nav />}
      >
        {SETTINGS_SECTIONS.map((s) => (
          <SegmentedControlItem
            key={s}
            render={
              <a
                aria-current={s === current ? "page" : undefined}
                href={hashFor("settings", s)}
              />
            }
          >
            {SECTIONS[s].label}
          </SegmentedControlItem>
        ))}
      </SegmentedControl>
      <Section />
    </Page>
  );
}
