import { describe, expect, it } from "vitest";
import {
  isCurrentVoiceOperation,
  isEventForActiveSession,
  recoveredRecordingReview,
  shouldAdoptWakingSession,
  shouldApplyVoiceProcessStatus,
  streamingTranscriptForReview,
  usesStreamingAsr,
  voiceOutputFlow,
  withReviewTimeout,
} from "./voiceReview";

describe("voice review async operations", () => {
  it.each([
    [true, false, { direct: true, format: false }],
    [true, true, { direct: true, format: true }],
    [false, false, { direct: false, format: false }],
    [false, true, { direct: false, format: true }],
  ])(
    "selects the output flow for direct=%s and format=%s",
    (directInsert, autoFormat, expected) => {
      expect(voiceOutputFlow({ directInsert, autoFormat })).toEqual(expected);
    },
  );

  it("accepts results only for the active session and epoch", () => {
    expect(isCurrentVoiceOperation(4, 4, "session-a", "session-a")).toBe(true);
    expect(isCurrentVoiceOperation(4, 5, "session-a", "session-a")).toBe(false);
    expect(isCurrentVoiceOperation(4, 4, "session-a", "session-b")).toBe(false);
    expect(isCurrentVoiceOperation(4, 4, "session-a", null)).toBe(false);
  });

  it("rejects late events from a previous or already cleared session", () => {
    expect(isEventForActiveSession("session-a", "session-a")).toBe(true);
    expect(isEventForActiveSession("session-a", "session-b")).toBe(false);
    expect(isEventForActiveSession("session-a", null)).toBe(false);
  });

  it("adopts a waking session only when idle or already on that session", () => {
    expect(shouldAdoptWakingSession("session-a", null)).toBe(true);
    expect(shouldAdoptWakingSession("session-a", "session-a")).toBe(true);
    expect(shouldAdoptWakingSession("session-b", "session-a")).toBe(false);
    expect(shouldAdoptWakingSession(undefined, null)).toBe(false);
  });

  it("still applies a failed status after the session id has been cleared", () => {
    expect(shouldApplyVoiceProcessStatus("failed", "session-a", null)).toBe(true);
    expect(shouldApplyVoiceProcessStatus("failed", "session-a", "session-a")).toBe(true);
    expect(shouldApplyVoiceProcessStatus("failed", "session-b", "session-a")).toBe(false);
    expect(shouldApplyVoiceProcessStatus("recognizing", "session-a", null)).toBe(false);
  });

  it("fails explicitly instead of leaving the review request pending", async () => {
    await expect(
      withReviewTimeout(new Promise(() => {}), 1),
    ).rejects.toMatchObject({ code: "review.timeout", retryable: true });
  });

  it("treats sherpa as a streaming engine when finishing a session", () => {
    expect(usesStreamingAsr("sherpa-streaming")).toBe(true);
    expect(usesStreamingAsr("qwen3-asr")).toBe(false);
    expect(usesStreamingAsr("faster-whisper")).toBe(false);
  });

  it("keeps visible streaming partials when finishing a session", () => {
    expect(streamingTranscriptForReview("", "嗯")).toBe("嗯");
    expect(streamingTranscriptForReview("打开语流", "")).toBe("打开语流");
    expect(streamingTranscriptForReview("打开", "打开语流")).toBe("打开语流");
    expect(streamingTranscriptForReview("第一句", "第二句")).toBe("第一句第二句");
  });

  it("preserves already recognized text when the backend review response is late", () => {
    expect(
      recoveredRecordingReview({
        sessionId: "session-a",
        text: "已经写入的文字",
        duration: 4,
        engine: "qwen3-asr",
        model: "qwen3-asr-1.7b",
      }),
    ).toMatchObject({
      sessionId: "session-a",
      text: "已经写入的文字",
      engine: "qwen3-asr",
    });
  });
});
