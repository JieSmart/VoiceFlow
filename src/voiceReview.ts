import type { RecordingReview } from "./types";

export function voiceOutputFlow(settings: {
  directInsert: boolean;
  autoFormat: boolean;
}) {
  return {
    direct: settings.directInsert,
    format: settings.autoFormat,
  } as const;
}

export function isCurrentVoiceOperation(
  operationEpoch: number,
  currentEpoch: number,
  sessionId: string,
  activeSessionId: string | null,
) {
  return operationEpoch === currentEpoch && sessionId === activeSessionId;
}

export function isEventForActiveSession(
  eventSessionId: string,
  activeSessionId: string | null,
) {
  return activeSessionId !== null && eventSessionId === activeSessionId;
}

export function shouldAdoptWakingSession(
  eventSessionId: string | undefined,
  activeSessionId: string | null,
) {
  return Boolean(eventSessionId) && (activeSessionId === null || activeSessionId === eventSessionId);
}

export function shouldApplyVoiceProcessStatus(
  phase: string,
  eventSessionId: string | undefined,
  activeSessionId: string | null,
) {
  if (phase === "waking") {
    return shouldAdoptWakingSession(eventSessionId, activeSessionId);
  }
  if (!eventSessionId) return true;
  if (isEventForActiveSession(eventSessionId, activeSessionId)) return true;
  return phase === "failed" && activeSessionId === null;
}

export const BUBBLE_FAILURE_HIDE_MS = 6_000;

export function usesStreamingAsr(provider: string) {
  return ["openai", "xunfei", "tencent", "sherpa-streaming"].includes(provider);
}

export async function withReviewTimeout<T>(
  request: Promise<T>,
  timeoutMs: number,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(
      () =>
        reject({
          code: "review.timeout",
          message: "结果卡生成超时，请重试",
          retryable: true,
          fallback: null,
        }),
      timeoutMs,
    );
  });
  try {
    return await Promise.race([request, timeout]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

export function streamingTranscriptForReview(
  committedText: string,
  partialText: string,
) {
  const committed = committedText.trim();
  const partial = partialText.trim();
  if (!committed) return partial;
  if (!partial) return committed;
  if (partial.startsWith(committed) || committed.endsWith(partial)) {
    return partial.length >= committed.length ? partial : committed;
  }
  return `${committed}${partial}`;
}

export function recoveredRecordingReview(input: {
  sessionId: string;
  text: string;
  duration: number;
  engine: string;
  model?: string;
}): RecordingReview {
  return {
    sessionId: input.sessionId,
    text: input.text,
    duration: input.duration,
    hotwords: [],
    engine: input.engine,
    model: input.model,
  };
}
