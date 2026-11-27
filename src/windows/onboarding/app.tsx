import type { OnboardingStep, Platform, Settings } from "@/lib/ipc";

import { useCallback, useEffect, useMemo, useState } from "react";

import { cn } from "cn";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";

import { MicTest } from "@/components/mic-test";
import { usePermissions } from "@/components/permission-list";
import { Button } from "@/components/ui/button";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import { useSettings } from "@/hooks/use-settings";
import { detectPlatform, formatHotkey } from "@/lib/hotkey";
import { completeOnboarding, listModels, openMainWindow } from "@/lib/ipc";
import { ONBOARDING_STEPS } from "@/lib/ipc/types";

import { PolishStep, VoiceModelStep } from "./step-models";
import { PermissionsStep } from "./step-permissions";
import { ShortcutStep } from "./step-shortcut";
import { TryItStep } from "./step-try-it";
import { WelcomeStep } from "./step-welcome";
import { adjacentStep, fill, STEPS, type StepContext, stepGate } from "./steps";

const INTERACTIVE =
  "button, a, input, textarea, select, [contenteditable], [role]";

function StepProgress({ current }: { current: OnboardingStep }) {
  const currentIndex = ONBOARDING_STEPS.indexOf(current);
  return (
    <ol aria-label="Setup progress" className="flex items-center gap-1.5">
      {ONBOARDING_STEPS.map((id, index) => {
        let state = "upcoming";
        if (index < currentIndex) {
          state = "done";
        } else if (index === currentIndex) {
          state = "current";
        }
        return (
          <li
            aria-current={state === "current" ? "step" : undefined}
            className={cn(
              "h-1 rounded-full transition-[width,background-color] duration-300 ease-out motion-reduce:transition-none",
              state === "current" ? "w-6 bg-primary" : "w-3",
              state === "done" && "bg-foreground-muted",
              state === "upcoming" && "bg-accent"
            )}
            key={id}
          >
            <span className="sr-only">
              {STEPS[id].label}
              {state === "done" ? ", done" : null}
            </span>
          </li>
        );
      })}
    </ol>
  );
}

function StepBody({
  onHeard,
  onMicrophoneChange,
  permissions,
  platform,
  settings,
  step,
  update,
}: {
  onHeard: () => void;
  onMicrophoneChange: (microphone: string | null) => void;
  permissions: StepContext["permissions"];
  platform: Platform;
  settings: Settings;
  step: OnboardingStep;
  update: (patch: Partial<Settings>) => Promise<unknown>;
}) {
  switch (step) {
    case "welcome":
      return <WelcomeStep />;
    case "permissions":
      return <PermissionsStep permissions={permissions} />;
    case "microphone":
      return (
        <MicTest
          microphone={settings.microphone}
          onHeard={onHeard}
          onMicrophoneChange={onMicrophoneChange}
        />
      );
    case "shortcut":
      return (
        <ShortcutStep platform={platform} settings={settings} update={update} />
      );
    case "voiceModel":
      return <VoiceModelStep settings={settings} update={update} />;
    case "polish":
      return <PolishStep settings={settings} update={update} />;
    case "tryIt":
      return (
        <TryItStep platform={platform} settings={settings} update={update} />
      );
    default:
      return null;
  }
}

const SLIDE_PX = 16;

function Onboarding({ settings }: { settings: Settings }) {
  const { update } = useSettings();
  const platform = detectPlatform();
  const reduceMotion = useReducedMotion() ?? false;
  // Resumes where the user left off; saved on every move.
  const [step, setStep] = useState<OnboardingStep>(settings.onboardingStep);
  const [direction, setDirection] = useState<1 | -1>(1);
  const [heardSpeech, setHeardSpeech] = useState(false);
  const [finishing, setFinishing] = useState(false);
  const { permissions } = usePermissions({ poll: step === "permissions" });
  const models = useIpcQuery(listModels, ["models:changed"]);

  const context: StepContext = useMemo(
    () => ({
      heardSpeech,
      permissions,
      sttModel: settings.sttModel,
      voiceModels: models.data?.filter((model) => model.kind === "voice"),
    }),
    [heardSpeech, models.data, permissions, settings.sttModel]
  );
  const gate = stepGate(step, context);
  const nextStep = adjacentStep(step, 1, context);
  const previousStep = adjacentStep(step, -1, context);
  const definition = STEPS[step];

  const goTo = useCallback(
    (target: OnboardingStep | undefined, towards: 1 | -1) => {
      if (!target) {
        return;
      }
      setDirection(towards);
      setStep(target);
      update({ onboardingStep: target }).catch((error: unknown) => {
        console.error("Could not save the onboarding step", error);
      });
    },
    [update]
  );

  const goBack = useCallback(
    () => goTo(previousStep, -1),
    [goTo, previousStep]
  );
  const markHeard = useCallback(() => setHeardSpeech(true), []);
  const saveMicrophone = useCallback(
    (microphone: string | null) => {
      update({ microphone }).catch((error: unknown) => {
        console.error("Could not save the microphone", error);
      });
    },
    [update]
  );

  const finish = useCallback(async () => {
    setFinishing(true);
    try {
      await completeOnboarding();
      await openMainWindow();
    } catch (error) {
      console.error("Could not finish setup", error);
      setFinishing(false);
    }
  }, []);

  const primary = useCallback(() => {
    if (!gate.met || finishing) {
      return;
    }
    if (nextStep) {
      goTo(nextStep, 1);
    } else {
      finish();
    }
  }, [finish, finishing, gate.met, goTo, nextStep]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.key !== "Enter" ||
        event.isComposing ||
        event.defaultPrevented
      ) {
        return;
      }
      const { target } = event;
      if (target instanceof Element && target.closest(INTERACTIVE)) {
        return;
      }
      event.preventDefault();
      primary();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [primary]);

  const focusHeading = useCallback(
    (heading: HTMLHeadingElement | null) => {
      if (heading && step !== "tryIt") {
        heading.focus({ preventScroll: true });
      }
    },
    [step]
  );

  let primaryLabel = "Continue";
  if (step === "welcome") {
    primaryLabel = "Get started";
  } else if (step === "polish" && !settings.llmModel) {
    primaryLabel = "Skip for now";
  } else if (!nextStep) {
    primaryLabel = "Finish";
  }

  const values = {
    key: formatHotkey(settings.shortcuts.pushToTalk, platform),
    place: platform === "macos" ? "Mac" : "PC",
  };
  const offset = reduceMotion ? 0 : SLIDE_PX;
  const variants = {
    center: { opacity: 1, x: 0 },
    enter: (towards: 1 | -1) => ({ opacity: 0, x: towards * offset }),
    exit: (towards: 1 | -1) => ({ opacity: 0, x: -towards * offset }),
  };
  const index = ONBOARDING_STEPS.indexOf(step);

  return (
    <div className="flex h-svh flex-col overflow-hidden">
      <header
        className="flex h-12 shrink-0 items-center justify-center"
        data-tauri-drag-region
      >
        <nav className="pointer-events-none">
          <StepProgress current={step} />
        </nav>
      </header>

      <main className="relative flex min-h-0 flex-1 flex-col">
        <AnimatePresence custom={direction} initial={false} mode="wait">
          <motion.section
            animate="center"
            aria-labelledby="step-title"
            className="mx-auto flex min-h-0 w-full max-w-xl flex-1 flex-col gap-5 px-4 pt-4"
            custom={direction}
            exit="exit"
            initial="enter"
            key={step}
            transition={{ duration: 0.18, ease: "easeOut" }}
            variants={variants}
          >
            <div className="flex flex-col gap-2" data-tauri-drag-region>
              <p className="text-foreground-muted text-xs">
                Step {index + 1} of {ONBOARDING_STEPS.length}
              </p>
              <h1
                className="font-redaction text-4xl text-foreground outline-none"
                id="step-title"
                ref={focusHeading}
                tabIndex={-1}
              >
                {definition.title}
              </h1>
              <p className="text-foreground-muted text-sm">
                {fill(definition.description, values)}
              </p>
            </div>
            <div className="flex min-h-0 flex-1 flex-col">
              <StepBody
                onHeard={markHeard}
                onMicrophoneChange={saveMicrophone}
                permissions={permissions}
                platform={platform}
                settings={settings}
                step={step}
                update={update}
              />
            </div>
          </motion.section>
        </AnimatePresence>
      </main>

      <footer className="mx-auto flex w-full max-w-xl shrink-0 items-center gap-3 px-4 py-4">
        {previousStep ? (
          <Button onClick={goBack} type="button" variant="ghost">
            Back
          </Button>
        ) : null}
        <p
          aria-live="polite"
          className="ms-auto text-end text-foreground-muted text-xs"
        >
          {gate.met ? null : gate.reason}
        </p>
        <Button
          disabled={!gate.met}
          loading={finishing}
          onClick={primary}
          type="button"
          variant={
            step === "polish" && !settings.llmModel ? "outline" : "default"
          }
        >
          {primaryLabel}
        </Button>
      </footer>
    </div>
  );
}

export function App() {
  const { settings } = useSettings();
  return settings ? <Onboarding settings={settings} /> : null;
}
