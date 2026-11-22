import type { NoticeKind, PillRect } from "@/lib/ipc";
import type { MockControls } from "@/lib/ipc/mock";

import { useCallback, useEffect, useRef, useState } from "react";

import { cn } from "cn";

import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";

import { PILL_WINDOW } from "./model";

/**
 * Development only (`/pill.html?controls=1` in a browser): the pill in an
 * iframe the size of its native window, over a pretend desktop, with buttons
 * that drive the iframe's mock backend.
 */

type Backdrop = "light" | "dark";

const NOTICES: [NoticeKind, string][] = [
  ["cancelled", "Cancelled"],
  ["pasteFailed", "Paste failed"],
  ["noSpeech", "No speech"],
  ["noMicrophone", "No microphone"],
  ["stillProcessing", "Still processing"],
  ["noModel", "No model"],
  ["error", "Error"],
];

const NINETEEN_MINUTES_MS = 19 * 60_000;
const SUCCESS_AFTER_MS = 300;
const POLL_MS = 200;

function useMock(frame: React.RefObject<HTMLIFrameElement | null>) {
  return useCallback(
    (drive: (mock: MockControls) => void) => {
      const mock = frame.current?.contentWindow?.__tinyWisprMock;
      if (mock) {
        drive(mock);
      }
    },
    [frame]
  );
}

export function Harness() {
  const frame = useRef<HTMLIFrameElement>(null);
  const drive = useMock(frame);
  const [backdrop, setBackdrop] = useState<Backdrop>("light");
  const [showBounds, setShowBounds] = useState(true);
  const [showRegions, setShowRegions] = useState(false);
  const [regions, setRegions] = useState<readonly PillRect[]>([]);

  useEffect(() => {
    if (!showRegions) {
      return;
    }
    const timer = setInterval(() => {
      const next = frame.current?.contentWindow?.__tinyWisprMock?.hitRegions;
      if (next) {
        setRegions((previous) =>
          JSON.stringify(previous) === JSON.stringify(next) ? previous : next
        );
      }
    }, POLL_MS);
    return () => clearInterval(timer);
  }, [showRegions]);

  return (
    <div
      className={cn(
        "relative h-svh overflow-hidden",
        backdrop === "light" ? "bg-foreground" : "bg-background-muted"
      )}
      data-slot="pill-harness"
    >
      <PretendDesktop backdrop={backdrop} />
      <div className="absolute inset-x-0 bottom-18 flex justify-center">
        <div
          className={cn(
            "relative",
            showBounds && "outline outline-dashed outline-primary/60"
          )}
          style={PILL_WINDOW}
        >
          <iframe
            className="block size-full border-0 bg-transparent"
            ref={frame}
            src="/pill.html"
            title="Pill window"
          />
          {showRegions
            ? regions.map((rect) => (
                <span
                  className="pointer-events-none absolute border border-info bg-info/20"
                  key={`${rect.x},${rect.y},${rect.width},${rect.height}`}
                  style={{
                    height: rect.height,
                    left: rect.x,
                    top: rect.y,
                    width: rect.width,
                  }}
                />
              ))
            : null}
        </div>
      </div>
      <Controls
        backdrop={backdrop}
        drive={drive}
        onBackdropChange={setBackdrop}
        onShowBoundsChange={setShowBounds}
        onShowRegionsChange={setShowRegions}
        showBounds={showBounds}
        showRegions={showRegions}
      />
    </div>
  );
}

function PretendDesktop({ backdrop }: { backdrop: Backdrop }) {
  const light = backdrop === "light";
  return (
    <div aria-hidden="true" className="pointer-events-none absolute inset-0">
      <div
        className={cn(
          "absolute inset-x-12 top-12 bottom-40 rounded-xl border shadow-2xl",
          light ? "border-background/10 bg-foreground" : "bg-background"
        )}
      >
        <div className="flex flex-col gap-3 p-10 ps-80">
          {[64, 80, 72, 40, 76, 58].map((width) => (
            <span
              className={cn(
                "block h-2 rounded-full",
                light ? "bg-background/10" : "bg-foreground/10"
              )}
              key={width}
              style={{ width: `${width}%` }}
            />
          ))}
        </div>
      </div>
      <div
        className={cn(
          "absolute bottom-2 left-1/2 h-14 w-md -translate-x-1/2 rounded-2xl border",
          light
            ? "border-background/10 bg-background/10"
            : "border-foreground/10 bg-foreground/5"
        )}
      />
    </div>
  );
}

function Controls({
  backdrop,
  drive,
  onBackdropChange,
  onShowBoundsChange,
  onShowRegionsChange,
  showBounds,
  showRegions,
}: {
  backdrop: Backdrop;
  drive: (drive: (mock: MockControls) => void) => void;
  onBackdropChange: (backdrop: Backdrop) => void;
  onShowBoundsChange: (show: boolean) => void;
  onShowRegionsChange: (show: boolean) => void;
  showBounds: boolean;
  showRegions: boolean;
}) {
  const [simulateSpeech, setSimulateSpeech] = useState(true);
  const [level, setLevel] = useState(0.5);
  const [showAlways, setShowAlways] = useState(true);
  const onLevelChange = useCallback((value: number | readonly number[]) => {
    setLevel(typeof value === "number" ? value : (value[0] ?? 0));
  }, []);
  const toggleBackdrop = useCallback(
    () => onBackdropChange(backdrop === "light" ? "dark" : "light"),
    [backdrop, onBackdropChange]
  );
  const unhide = useCallback(
    () => drive((mock) => mock.settings({ pillHiddenUntil: null })),
    [drive]
  );

  useEffect(() => {
    drive((mock) => mock.level(simulateSpeech ? null : level));
  }, [drive, level, simulateSpeech]);

  useEffect(() => {
    drive((mock) => mock.settings({ showPillAlways: showAlways }));
  }, [drive, showAlways]);

  const states: [string, (mock: MockControls) => void][] = [
    ["Idle", (mock) => mock.setPhase("idle")],
    ["Hold", (mock) => mock.setPhase("listening", { mode: "hold" })],
    ["Hands-free", (mock) => mock.setPhase("listening", { mode: "handsFree" })],
    [
      "Hands-free 0:42",
      (mock) =>
        mock.setPhase("listening", { elapsedMs: 42_000, mode: "handsFree" }),
    ],
    ["Transcribing", (mock) => mock.setPhase("processing")],
    [
      "Polishing",
      (mock) => mock.setPhase("processing", { stage: "polishing" }),
    ],
    [
      "Success",
      (mock) => {
        mock.setPhase("processing");
        setTimeout(() => mock.setPhase("idle"), SUCCESS_AFTER_MS);
      },
    ],
  ];

  const notices: [string, (mock: MockControls) => void][] = [
    ...NOTICES.map(([kind, label]): [string, (mock: MockControls) => void] => [
      label,
      (mock) => mock.notice(kind),
    ]),
    [
      "19-minute warning",
      (mock) => {
        mock.setPhase("listening", {
          elapsedMs: NINETEEN_MINUTES_MS,
          mode: "handsFree",
        });
        mock.notice("timeLimitWarning");
      },
    ],
  ];

  return (
    <div className="absolute top-4 left-4 flex w-72 flex-col gap-4 rounded-xl border bg-background p-4 text-foreground text-sm shadow-lg">
      <ButtonGroup drive={drive} items={states} title="State" />
      <ButtonGroup drive={drive} items={notices} title="Notice" />
      <section className="flex flex-col gap-2">
        <h2 className="font-medium text-foreground-muted text-xs">Input</h2>
        <Label>
          <Switch
            checked={simulateSpeech}
            onCheckedChange={setSimulateSpeech}
          />
          Simulate speech
        </Label>
        <Slider
          aria-label="Input level"
          disabled={simulateSpeech}
          max={1}
          min={0}
          onValueChange={onLevelChange}
          step={0.01}
          value={level}
        />
      </section>
      <section className="flex flex-col gap-2">
        <h2 className="font-medium text-foreground-muted text-xs">View</h2>
        <Label>
          <Switch checked={showAlways} onCheckedChange={setShowAlways} />
          Show the pill at all times
        </Label>
        <Label>
          <Switch checked={showBounds} onCheckedChange={onShowBoundsChange} />
          Window bounds
        </Label>
        <Label>
          <Switch checked={showRegions} onCheckedChange={onShowRegionsChange} />
          Hit regions
        </Label>
        <div className="flex flex-wrap gap-1.5">
          <Button
            onClick={toggleBackdrop}
            size="xs"
            type="button"
            variant="secondary"
          >
            {backdrop === "light" ? "Dark desktop" : "Light desktop"}
          </Button>
          <Button onClick={unhide} size="xs" type="button" variant="secondary">
            Unhide
          </Button>
        </div>
      </section>
    </div>
  );
}

function ButtonGroup({
  drive,
  items,
  title,
}: {
  drive: (drive: (mock: MockControls) => void) => void;
  items: [string, (mock: MockControls) => void][];
  title: string;
}) {
  return (
    <section className="flex flex-col gap-2">
      <h2 className="font-medium text-foreground-muted text-xs">{title}</h2>
      <div className="flex flex-wrap gap-1.5">
        {items.map(([label, action]) => (
          <DriveButton action={action} drive={drive} key={label}>
            {label}
          </DriveButton>
        ))}
      </div>
    </section>
  );
}

function DriveButton({
  action,
  children,
  drive,
}: {
  action: (mock: MockControls) => void;
  children: React.ReactNode;
  drive: (drive: (mock: MockControls) => void) => void;
}) {
  const onClick = useCallback(() => drive(action), [action, drive]);
  return (
    <Button onClick={onClick} size="xs" type="button" variant="secondary">
      {children}
    </Button>
  );
}
