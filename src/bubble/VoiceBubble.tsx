import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  getCurrentWindow,
  PhysicalPosition,
  PhysicalSize,
} from "@tauri-apps/api/window";
import clsx from "clsx";
import { isTauri } from "../api";
import { useAppStore } from "../store";
import type { AppSettings, RecordingReview, VoiceProcessStatus } from "../types";
import {
  bubbleShouldActivateFromHidden,
  bubbleStyleWindowSize,
  bubbleVisualState,
  bubbleWindowTransitionPlan,
  runBubbleWindowTransition,
  type BubbleWindowSize,
  voiceProcessCopy,
} from "../voiceProcess";
import { WaveLogo } from "../ui/primitives";
import { formatWakeHint } from "../hotkeys";
import { usesStreamingAsr } from "../voiceReview";
import { selectedBubbleStyle } from "./selectedStyle";
import { BubbleA, BubbleB, BubbleC } from "./variants";
import type { VoiceSession } from "../voice/types";

type BubblePresentation = {
  style: AppSettings["bubbleStyle"];
  phase: VoiceProcessStatus["phase"];
  hasSession: boolean;
  review: RecordingReview | null;
  formattedText: string;
  formatting: boolean;
  notice?: string;
};

function bubblePresentationVisual(presentation: BubblePresentation) {
  return bubbleVisualState(
    presentation.phase,
    Boolean(presentation.review),
    Boolean(presentation.formattedText || presentation.formatting),
  );
}

function bubblePresentationSize(
  presentation: BubblePresentation,
): BubbleWindowSize {
  return bubbleStyleWindowSize(
    presentation.style,
    bubblePresentationVisual(presentation),
  );
}

function bubblePresentationKey(presentation: BubblePresentation) {
  const visual = bubblePresentationVisual(presentation);
  return `${presentation.style}:${visual.stage}:${visual.expandedReview}`;
}

export function VoiceBubble({ voice }: { voice: VoiceSession }) {
  const session = useAppStore((s) => s.session),
    partial = useAppStore((s) => s.partialText),
    committed = useAppStore((s) => s.committedText),
    level = useAppStore((s) => s.audioLevel),
    notice = useAppStore((s) => s.notice),
    phase = useAppStore((s) => s.voicePhase),
    configuredStyle = useAppStore((s) => s.settings.bubbleStyle),
    settings = useAppStore((s) => s.settings);
  const style = selectedBubbleStyle({
    ...useAppStore.getState().settings,
    bubbleStyle: configuredStyle,
  });
  const hasSession = Boolean(session);
  const press = useRef<{
    x: number;
    y: number;
    dragging: boolean;
  } | null>(null);
  const resizeQueue = useRef<Promise<void>>(Promise.resolve());
  const transitionGeneration = useRef(0);
  const transitioning = useRef(false);
  const initializedWindowSize = useRef(false);
  const requestedPresentation = useMemo<BubblePresentation>(() => ({
    style,
    phase,
    hasSession,
    review: voice.review,
    formattedText: voice.formattedText,
    formatting: voice.formatting,
    notice,
  }), [
    style,
    phase,
    hasSession,
    voice.review,
    voice.formattedText,
    voice.formatting,
    notice,
  ]);
  const [presentation, setPresentation] = useState(requestedPresentation);
  const presentationRef = useRef(presentation);
  const [transitionMode, setTransitionMode] = useState<
    "prepared" | "entering" | "leaving" | null
  >(null);
  const liveText = `${committed}${partial}`;
  const wakeHint = formatWakeHint(settings.hotkeys.wake);
  const [statusTitle] = voiceProcessCopy(
    presentation.phase,
    Boolean(committed || partial),
    {
      streaming: usesStreamingAsr(settings.asrProvider),
      wake: settings.hotkeys.wake,
    },
  );
  const canStop = Boolean(session) && phase === "listening";
  const canStart = !session && phase === "idle";
  const canDismissFailure = phase === "failed" && !voice.review;
  const interaction = {
    role: "button" as const,
    tabIndex: -1,
    "aria-label": canStop
      ? "结束录音"
      : canStart
        ? "开始录音"
        : canDismissFailure
          ? "关闭失败提示"
          : statusTitle,
    onPointerDown: (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      event.preventDefault();
      press.current = { x: event.clientX, y: event.clientY, dragging: false };
      event.currentTarget.setPointerCapture(event.pointerId);
    },
    onPointerMove: (event: React.PointerEvent<HTMLDivElement>) => {
      const current = press.current;
      if (!current || current.dragging) return;
      if (Math.hypot(event.clientX - current.x, event.clientY - current.y) < 6)
        return;
      current.dragging = true;
      if (isTauri()) void getCurrentWindow().startDragging();
    },
    onPointerUp: (event: React.PointerEvent<HTMLDivElement>) => {
      const current = press.current;
      press.current = null;
      if (event.currentTarget.hasPointerCapture(event.pointerId))
        event.currentTarget.releasePointerCapture(event.pointerId);
      if (!current?.dragging) {
        if (canStop) void voice.finish();
        else if (canStart) void voice.begin();
        else if (canDismissFailure) voice.dismissFailure();
      }
    },
    onPointerCancel: () => {
      press.current = null;
    },
    onKeyDown: (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      if (canStop) void voice.finish();
      else if (canStart) void voice.begin();
      else if (canDismissFailure) voice.dismissFailure();
    },
  };
  const dragInteraction = {
    onPointerDown: interaction.onPointerDown,
    onPointerMove: interaction.onPointerMove,
    onPointerUp: interaction.onPointerUp,
    onPointerCancel: interaction.onPointerCancel,
  };

  const serializedWindowOperation = useCallback(
    async <T,>(operation: () => Promise<T> | T) => {
      const previous = resizeQueue.current;
      let release!: () => void;
      resizeQueue.current = new Promise<void>((resolve) => {
        release = resolve;
      });
      await previous;
      try {
        return await operation();
      } finally {
        release();
      }
    },
    [],
  );

  const resizeNativeWindow = useCallback(async (target: BubbleWindowSize) => {
    await serializedWindowOperation(async () => {
      const window = getCurrentWindow();
      const scale = await window.scaleFactor();
      const [position, outerSize, innerSize] = await Promise.all([
        window.outerPosition(),
        window.outerSize(),
        window.innerSize(),
      ]);
      const nextInnerWidth = Math.round(target.width * scale);
      const nextInnerHeight = Math.round(target.height * scale);
      if (
        innerSize.width === nextInnerWidth &&
        innerSize.height === nextInnerHeight
      ) return;

      const centerX = position.x + outerSize.width / 2;
      const bottom = position.y + outerSize.height;
      await window.setSize(new PhysicalSize(nextInnerWidth, nextInnerHeight));
      const resizedOuterSize = await window.outerSize();
      await window.setPosition(new PhysicalPosition(
        Math.round(centerX - resizedOuterSize.width / 2),
        Math.round(bottom - resizedOuterSize.height),
      ));
    });
  }, [serializedWindowOperation]);

  const waitForLayoutPaint = useCallback(async () => {
    await new Promise<void>((resolve) => {
      if (typeof globalThis.requestAnimationFrame !== "function") {
        globalThis.setTimeout(resolve, 0);
        return;
      }
      globalThis.requestAnimationFrame(() => {
        globalThis.requestAnimationFrame(() => resolve());
      });
    });
  }, []);

  const waitForContentAnimation = useCallback(async (
    mode: "entering" | "leaving",
  ) => {
    const reducedMotion = globalThis.matchMedia?.(
      "(prefers-reduced-motion: reduce)",
    ).matches ?? false;
    await new Promise<void>((resolve) => globalThis.setTimeout(resolve, 0));
    if (reducedMotion) return;
    const element = document.querySelector<HTMLElement>(
      `.bubble-presentation.${mode}`,
    );
    if (!element) return;
    await new Promise<void>((resolve) => {
      const finish = () => {
        element.removeEventListener("animationend", onAnimationEnd);
        resolve();
      };
      const onAnimationEnd = (event: AnimationEvent) => {
        if (event.target !== element) return;
        clearTimeout(timeout);
        finish();
      };
      const timeout = globalThis.setTimeout(finish, mode === "leaving" ? 260 : 750);
      element.addEventListener("animationend", onAnimationEnd);
    });
  }, []);

  useEffect(() => {
    const generation = ++transitionGeneration.current;
    const isCurrent = () => generation === transitionGeneration.current;
    const commit = () => {
      if (!isCurrent()) throw new Error("bubble-transition-cancelled");
      presentationRef.current = requestedPresentation;
      setPresentation(requestedPresentation);
    };
    if (!isTauri()) {
      commit();
      setTransitionMode(null);
      return;
    }

    const run = async () => {
      transitioning.current = true;
      try {
        if (!initializedWindowSize.current) {
          initializedWindowSize.current = true;
          await resizeNativeWindow(bubblePresentationSize(requestedPresentation));
          if (!isCurrent()) return;
        }
        const current = presentationRef.current;
        if (
          bubblePresentationKey(current) ===
          bubblePresentationKey(requestedPresentation)
        ) {
          commit();
          return;
        }
        const activating = bubbleShouldActivateFromHidden(
          bubblePresentationVisual(current).stage,
          bubblePresentationVisual(requestedPresentation).stage,
        );
        if (activating) {
          await resizeNativeWindow(bubblePresentationSize(requestedPresentation));
          if (!isCurrent()) return;
          commit();
          setTransitionMode("prepared");
          await new Promise<void>((resolve) => globalThis.setTimeout(resolve, 0));
          if (!isCurrent()) return;
          await getCurrentWindow().show();
          await waitForLayoutPaint();
          if (!isCurrent()) return;
          setTransitionMode("entering");
          await waitForContentAnimation("entering");
          if (isCurrent()) setTransitionMode(null);
          return;
        }
        const plan = bubbleWindowTransitionPlan(
          bubblePresentationSize(current),
          bubblePresentationSize(requestedPresentation),
        );
        await runBubbleWindowTransition(plan, {
          resize: async (size) => {
            if (!isCurrent()) throw new Error("bubble-transition-cancelled");
            await resizeNativeWindow(size);
            if (!isCurrent()) throw new Error("bubble-transition-cancelled");
          },
          leave: async () => {
            if (!isCurrent()) throw new Error("bubble-transition-cancelled");
            setTransitionMode("leaving");
            await waitForContentAnimation("leaving");
            if (!isCurrent()) throw new Error("bubble-transition-cancelled");
          },
          present: () => {
            commit();
            setTransitionMode("entering");
          },
          enter: async () => {
            await waitForContentAnimation("entering");
            if (!isCurrent()) throw new Error("bubble-transition-cancelled");
            setTransitionMode(null);
          },
        });
      } catch (error) {
        if ((error as Error).message !== "bubble-transition-cancelled") throw error;
      } finally {
        if (isCurrent()) {
          transitioning.current = false;
          setTransitionMode(null);
        }
      }
    };
    void run();
  }, [
    requestedPresentation,
    resizeNativeWindow,
    waitForLayoutPaint,
    waitForContentAnimation,
  ]);

  useEffect(() => () => {
    transitionGeneration.current += 1;
  }, []);

  const displayedVoice = {
    ...voice,
    review: presentation.review,
    formattedText: presentation.formattedText,
    formatting: presentation.formatting,
  };
  let content: React.ReactNode;
  if (!presentation.hasSession && presentation.phase === "idle") {
    content = (
      <div className="bubble-idle" {...interaction}>
        <WaveLogo />
        <b>语流已待命</b>
        <small>按 {wakeHint} 开始</small>
      </div>
    );
  } else {
    const Variant = presentation.style === "A"
      ? BubbleA
      : presentation.style === "B"
        ? BubbleB
        : BubbleC;
    content = (
      <Variant
        voice={displayedVoice}
        interaction={interaction}
        dragInteraction={dragInteraction}
        text={liveText}
        level={level}
        phase={presentation.phase}
        notice={presentation.notice}
      />
    );
  }
  return (
    <div
      key={bubblePresentationKey(presentation)}
      className={clsx(
        "bubble-presentation",
        `style-${presentation.style}`,
        `stage-${bubblePresentationVisual(presentation).stage}`,
        transitionMode,
      )}
    >
      {content}
    </div>
  );
}
