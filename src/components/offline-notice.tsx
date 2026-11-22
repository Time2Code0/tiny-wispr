import { WifiOff01Icon } from "@hugeicons/core-free-icons";

import { Alert, AlertAction, AlertDescription } from "@/components/ui/alert";
import { Icon } from "@/components/ui/icon";

/** Shown above model lists while "Stay offline" keeps downloads from starting. */
export function OfflineNotice({ action }: { action: React.ReactNode }) {
  return (
    <Alert role="status">
      <Icon aria-hidden="true" icon={WifiOff01Icon} />
      <AlertDescription>
        You're offline by choice. Turn off Stay offline in Privacy to download.
      </AlertDescription>
      <AlertAction>{action}</AlertAction>
    </Alert>
  );
}
