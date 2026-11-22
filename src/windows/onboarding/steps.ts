import type { ModelInfo, OnboardingStep, Permissions } from "@/lib/ipc";

import { requiredPermissions } from "@/components/permission-list";
import { ONBOARDING_STEPS } from "@/lib/ipc/types";

export interface StepDefinition {
  description: string;
  label: string;
  title: string;
}

/** `{place}` is "Mac" or "PC"; `{key}` is the push-to-talk shortcut. */
export const STEPS: Record<OnboardingStep, StepDefinition> = {
  microphone: {
    description: "Say a few words. The bars move when Tiny Wispr hears you.",
    label: "Microphone",
    title: "Test your microphone",
  },
  permissions: {
    description:
      "Tiny Wispr needs a few permissions to hear you and put the text where you type.",
    label: "Permissions",
    title: "Allow access",
  },
  polish: {
    description:
      "A local language model follows corrections like “actually, make that 3” and smooths your wording. It takes disk space and adds a second or two to each dictation.",
    label: "Polish",
    title: "Polish your text",
  },
  shortcut: {
    description: "Hold to talk. Double-tap to keep listening hands-free.",
    label: "Shortcut",
    title: "Your shortcut",
  },
  tryIt: {
    description:
      "Click in the box, then hold {key} and say something. Release to see the text.",
    label: "Try it",
    title: "Try it",
  },
  voiceModel: {
    description:
      "A voice model turns speech into text on this {place}. We picked one that suits it.",
    label: "Voice model",
    title: "Choose a voice model",
  },
  welcome: {
    description:
      "Hold a key, speak, and your words appear wherever you type. Everything runs on this {place}. Nothing you say leaves it.",
    label: "Welcome",
    title: "Welcome to Tiny Wispr",
  },
};

const PLACEHOLDER = /\{(\w+)\}/g;

export function fill(text: string, values: Record<string, string>): string {
  return text.replace(
    PLACEHOLDER,
    (match, name: string) => values[name] ?? match
  );
}

/** What the steps need to know to decide whether the user may move on. */
export interface StepContext {
  /** Speech-level input was seen during the microphone test. */
  heardSpeech: boolean;
  permissions: Permissions | undefined;
  sttModel: string | null;
  voiceModels: readonly ModelInfo[] | undefined;
}

export type StepGate = { met: true } | { met: false; reason: string };

const MET: StepGate = { met: true };

const PERMISSION_NAMES = {
  accessibility: "Accessibility",
  inputMonitoring: "Input Monitoring",
  microphone: "microphone access",
} as const;

export function permissionsGranted(
  permissions: Permissions | undefined
): boolean {
  return (
    permissions !== undefined &&
    requiredPermissions(permissions).every(
      (kind) => permissions[kind] === "granted"
    )
  );
}

/** Whether `step` lets the user continue, and if not, what to do first. */
export function stepGate(step: OnboardingStep, context: StepContext): StepGate {
  switch (step) {
    case "permissions": {
      const { permissions } = context;
      if (!permissions) {
        return { met: false, reason: "Checking permissions" };
      }
      const missing = requiredPermissions(permissions).find(
        (kind) => permissions[kind] !== "granted"
      );
      return missing
        ? {
            met: false,
            reason: `Allow ${PERMISSION_NAMES[missing]} to continue`,
          }
        : MET;
    }
    case "microphone":
      return context.heardSpeech
        ? MET
        : { met: false, reason: "Say something to continue" };
    case "voiceModel": {
      const selected = context.voiceModels?.find(
        (model) => model.id === context.sttModel
      );
      if (selected?.status.state === "downloaded") {
        return MET;
      }
      const anyDownloaded = context.voiceModels?.some(
        (model) => model.status.state === "downloaded"
      );
      return {
        met: false,
        reason: anyDownloaded
          ? "Choose a downloaded model to continue"
          : "Download a voice model to continue",
      };
    }
    default:
      return MET;
  }
}

/** Steps the user never needs to see, given what is already set up. */
function skipped(step: OnboardingStep, context: StepContext): boolean {
  return step === "permissions" && permissionsGranted(context.permissions);
}

/**
 * The step after (`direction` 1) or before (-1) `step`, passing over steps
 * that are already done and need no input, such as granted permissions.
 */
export function adjacentStep(
  step: OnboardingStep,
  direction: 1 | -1,
  context: StepContext
): OnboardingStep | undefined {
  let index = ONBOARDING_STEPS.indexOf(step) + direction;
  while (index >= 0 && index < ONBOARDING_STEPS.length) {
    const candidate = ONBOARDING_STEPS[index];
    if (candidate && !skipped(candidate, context)) {
      return candidate;
    }
    index += direction;
  }
  return undefined;
}
