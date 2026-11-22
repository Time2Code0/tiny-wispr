import { useId } from "react";

import { cn } from "cn";

/** The frame every hub page shares: a serif title, a line of context, content. */
export function Page({
  title,
  description,
  children,
  className,
}: {
  children?: React.ReactNode;
  className?: string;
  description?: React.ReactNode;
  title: React.ReactNode;
}) {
  return (
    <div
      className={cn(
        "mx-auto flex w-full max-w-3xl flex-1 flex-col gap-10 px-10 pb-16",
        className
      )}
    >
      <header className="flex flex-col gap-2">
        <h1 className="font-redaction text-4xl text-foreground">{title}</h1>
        {description ? (
          <p className="text-pretty text-foreground-muted text-sm">
            {description}
          </p>
        ) : null}
      </header>
      {children}
    </div>
  );
}

/** A titled block of a page. */
export function Section({
  title,
  description,
  action,
  children,
  className,
}: {
  action?: React.ReactNode;
  children?: React.ReactNode;
  className?: string;
  description?: React.ReactNode;
  title: React.ReactNode;
}) {
  const id = useId();
  return (
    <section
      aria-labelledby={id}
      className={cn("flex flex-col gap-4", className)}
    >
      <div className="flex items-end justify-between gap-4">
        <div className="flex min-w-0 flex-col gap-1">
          <h2 className="font-medium text-base text-foreground" id={id}>
            {title}
          </h2>
          {description ? (
            <p className="text-pretty text-foreground-muted text-sm">
              {description}
            </p>
          ) : null}
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}

/** The id SettingRow gives its description, for the control's `aria-describedby`. */
export const descriptionId = (controlId: string) => `${controlId}-description`;

/** One setting: what it is and what it does on the left, its control on the right. */
export function SettingRow({
  label,
  description,
  control,
  htmlFor,
  className,
}: {
  className?: string;
  control: React.ReactNode;
  description?: React.ReactNode;
  /** The control's id, so clicking the label works the control. */
  htmlFor?: string;
  label: React.ReactNode;
}) {
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-6 px-4 py-3.5",
        className
      )}
    >
      <div className="flex min-w-0 flex-col gap-0.5">
        {htmlFor ? (
          <label className="text-foreground text-sm" htmlFor={htmlFor}>
            {label}
          </label>
        ) : (
          <span className="text-foreground text-sm">{label}</span>
        )}
        {description ? (
          <p
            className="text-pretty text-foreground-muted text-xs"
            id={htmlFor ? descriptionId(htmlFor) : undefined}
          >
            {description}
          </p>
        ) : null}
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  );
}

/** Setting rows in one hairline card, divided by rules. */
export function SettingGroup({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-col divide-y rounded-xl border bg-background",
        className
      )}
    >
      {children}
    </div>
  );
}
