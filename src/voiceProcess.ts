import { formatWakeHint } from "./hotkeys";
import type { AppSettings, VoiceProcessPhase } from "./types";

export type BubbleAStage =
  | "idle"
  | "recording"
  | "processing"
  | "failed"
  | "review";

export interface BubbleWindowSize {
  width: number;
  height: number;
}

export interface BubbleVisualState {
  stage: BubbleAStage;
  expandedReview: boolean;
}

export interface BubbleWindowTransitionPlan {
  currentSize: BubbleWindowSize;
  targetSize: BubbleWindowSize;
  envelopeSize: BubbleWindowSize;
  expandFirst: boolean;
  shrinkAfter: boolean;
}

export interface BubbleTransitionActions {
  resize: (size: BubbleWindowSize) => Promise<void>;
  leave: () => Promise<void>;
  present: () => Promise<void> | void;
  enter: () => Promise<void>;
}

export function bubbleShouldActivateFromHidden(
  currentStage: BubbleAStage,
  nextStage: BubbleAStage,
) {
  return currentStage !== nextStage && nextStage !== "idle" && (
    currentStage === "idle" || currentStage === "failed"
  );
}

export function bubbleAStage(
  phase: VoiceProcessPhase,
  hasReview: boolean,
): BubbleAStage {
  if (hasReview) return "review";
  if (phase === "idle") return "idle";
  if (phase === "listening") return "recording";
  if (phase === "failed") return "failed";
  return "processing";
}

export function bubbleAWindowSize(
  stage: BubbleAStage,
  expandedReview = false,
): BubbleWindowSize {
  switch (stage) {
    case "idle":
      return { width: 390, height: 78 };
    case "processing":
      return { width: 340, height: 92 };
    case "failed":
      return { width: 420, height: 132 };
    case "recording":
      return { width: 620, height: 116 };
    case "review":
      return { width: 620, height: expandedReview ? 420 : 360 };
  }
}

export function bubbleVisualState(
  phase: VoiceProcessPhase,
  hasReview: boolean,
  expandedReview = false,
): BubbleVisualState {
  return {
    stage: bubbleAStage(phase, hasReview),
    expandedReview: hasReview && expandedReview,
  };
}

export function bubbleVisualWindowSize(
  visual: BubbleVisualState,
): BubbleWindowSize {
  return bubbleAWindowSize(visual.stage, visual.expandedReview);
}

export function bubbleStyleWindowSize(
  style: AppSettings["bubbleStyle"],
  visual: BubbleVisualState,
): BubbleWindowSize {
  if (style === "A") return bubbleVisualWindowSize(visual);

  const { stage, expandedReview } = visual;
  if (stage === "idle") return { width: 390, height: 78 };

  if (style === "B") {
    switch (stage) {
      case "recording":
        return { width: 600, height: 370 };
      case "processing":
        return { width: 600, height: 230 };
      case "failed":
        return { width: 580, height: 190 };
      case "review":
        return { width: 640, height: expandedReview ? 460 : 400 };
    }
  }

  switch (stage) {
    case "recording":
      return { width: 640, height: 210 };
    case "processing":
      return { width: 560, height: 170 };
    case "failed":
      return { width: 580, height: 190 };
    case "review":
      return { width: 660, height: expandedReview ? 460 : 400 };
  }
}

export function bubbleWindowTransitionPlan(
  currentSize: BubbleWindowSize,
  targetSize: BubbleWindowSize,
): BubbleWindowTransitionPlan {
  const envelopeSize = {
    width: Math.max(currentSize.width, targetSize.width),
    height: Math.max(currentSize.height, targetSize.height),
  };
  return {
    currentSize,
    targetSize,
    envelopeSize,
    expandFirst:
      envelopeSize.width !== currentSize.width ||
      envelopeSize.height !== currentSize.height,
    shrinkAfter:
      envelopeSize.width !== targetSize.width ||
      envelopeSize.height !== targetSize.height,
  };
}

/**
 * Keeps native bounds changes out of the visible animation. The outgoing DOM
 * leaves first, the native window snaps while transparent, and only then does
 * the incoming DOM animate.
 */
export async function runBubbleWindowTransition(
  plan: BubbleWindowTransitionPlan,
  actions: BubbleTransitionActions,
) {
  await actions.leave();
  await actions.resize(plan.targetSize);
  await actions.present();
  await actions.enter();
}

export function voiceProcessCopy(
  phase: VoiceProcessPhase,
  hasTranscript: boolean,
  options: { streaming?: boolean; wake?: string } = {},
) {
  const wake = formatWakeHint(options.wake || "Ctrl+Shift+Space");
  switch (phase) {
    case "waking":
      return ["正在唤醒", "正在连接麦克风"] as const;
    case "listening":
      return [
        "正在聆听",
        hasTranscript ? "正在实时识别" : "等待你说话",
      ] as const;
    case "recognizing":
      return [
        "正在识别",
        options.streaming
          ? "正在等待最终结果，最长约 22 秒"
          : "正在生成最终文字，最长等待 60 秒",
      ] as const;
    case "transforming":
      return ["正在整理", "正在处理识别结果"] as const;
    case "inserting":
      return ["正在写入", "正在写回原输入框"] as const;
    case "completed":
      return ["已输入", "文字已写入"] as const;
    case "failed":
      return ["处理失败", `点击关闭，或按 ${wake} 重试`] as const;
    default:
      return ["语流已待命", `按 ${wake} 开始`] as const;
  }
}
