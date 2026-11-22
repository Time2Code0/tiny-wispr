import type { Permissions } from "@/lib/ipc";

import { PermissionList } from "@/components/permission-list";
import { Spinner } from "@/components/ui/loading";

import { permissionsGranted } from "./steps";

export function PermissionsStep({
  permissions,
}: {
  permissions: Permissions | undefined;
}) {
  if (!permissions) {
    return <Spinner className="text-foreground-muted" />;
  }
  return (
    <div className="flex flex-col gap-4">
      <PermissionList permissions={permissions} />
      <p aria-live="polite" className="text-foreground-muted text-xs">
        {permissionsGranted(permissions)
          ? "All set. You can change these later in System Settings."
          : "Tiny Wispr only listens while you dictate."}
      </p>
    </div>
  );
}
