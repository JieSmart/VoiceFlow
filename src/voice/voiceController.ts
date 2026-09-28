import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  cancelRecording,
  captureFocusTarget,
  closeBubble as closeBubbleBackend,
  commitRecordingReview,
  injectSegment,
  isTauri,
  listPrompts,
  onEvent,
  publishVoiceStatus,
  recoverFailedSession,
  requestVoiceStop,
  startRecording,
  stopRecording,
  stopRecordingForReview,
  streamLlm,
  transcribeFinal,
  writeDiagnosticLog,
} from "../api";
import { playRecordingCue, shouldPlayRecordingCueInWindow } from "../cueSounds";
import { isBubbleWindow as detectBubbleWindow } from "../windowKind";
import { promptRequest, resolvePromptTemplate } from "../promptTemplates";
import { useAppStore } from "../store";
import {
  initialVoiceActivityState,
  markVoiceActivitySpeech,
  SILENCE_TIMEOUT_MS,
  shouldAutoFinishVoiceActivity,
} from "../voiceActivity";
import {
  BUBBLE_FAILURE_HIDE_MS,
  isCurrentVoiceOperation,
  isEventForActiveSession,
  recoveredRecordingReview,
  shouldAdoptWakingSession,
  shouldApplyVoiceProcessStatus,
  streamingTranscriptForReview,
  usesStreamingAsr,
  voiceOutputFlow,
  withReviewTimeout,
} from "../voiceReview";
import type {
  AppError,
  AppSettings,
  FocusTarget,
  RecordingReview,
  RecordingSession,
  TranscriptSegment,
  VoiceProcessStatus,
} from "../types";

const demoSentence =
  "帮我把这段话记到会议纪要里：语流的重点是热词命中率和识别延迟，明天上午十点和产品对齐 checklist。";

export function VoiceController() {
  const settings = useAppStore((s) => s.settings);
  const session = useAppStore((s) => s.session);
  const setSession = useAppStore((s) => s.setSession);
  const setVoice = useAppStore((s) => s.setVoice);
  const currentVoicePhase = useAppStore((s) => s.voicePhase);
  const isBubbleWindow = detectBubbleWindow();
  const demoTimer = useRef<number | null>(null);
  const hideTimer = useRef<number | null>(null);
  const finishing = useRef(false);
  const reviewActionPending = useRef(false);
  const operationEpoch = useRef(0);
  const liveText = useRef("");
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const activeSessionId = useRef<string | null>(null);
  const voiceActivity = useRef(initialVoiceActivityState());
  const [review, setReview] = useState<RecordingReview | null>(null);
  const [formattedText, setFormattedText] = useState("");
  const [formatting, setFormatting] = useState(false);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const reviewRef = useRef(review);
  const formattedTextRef = useRef(formattedText);
  reviewRef.current = review;
  formattedTextRef.current = formattedText;
  const scheduleBubbleHide = (delay: number) => {
    if (!isBubbleWindow) return;
    if (hideTimer.current) window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(async () => {
      setSession(null);
      setReview(null);
      setFormattedText("");
      setFormatting(false);
      setVoice({
        voicePhase: "idle",
        notice: undefined,
        partialText: "",
        committedText: "",
        audioLevel: 0,
      });
      if (isTauri()) {
        try {
          await closeBubbleBackend();
        } catch {
          await getCurrentWindow().hide();
        }
      }
      hideTimer.current = null;
    }, delay);
  };
  useEffect(() => {
    const cleanups: Promise<() => void>[] = [
      onEvent<{ sessionId: string; level: number; rms?: number }>(
        "audio-level",
        (p) => {
          if (p.sessionId !== activeSessionId.current) return;
          setVoice({ audioLevel: p.level });
          const rms = p.rms ?? p.level / 20;
          if (
            isBubbleWindow &&
            !finishing.current &&
            shouldAutoFinishVoiceActivity(
              settingsRef.current.autoEndOnSilence,
              voiceActivity.current,
              rms,
              performance.now(),
            )
          ) {
            void writeDiagnosticLog("info", "ui.auto_finish_silence", p.sessionId, {
              rms,
              noiseFloor: voiceActivity.current.noiseFloor,
              timeoutMs: SILENCE_TIMEOUT_MS,
            });
            void finish(p.sessionId);
          }
        },
      ),
      onEvent<RecordingSession>("recording-state", (p) => {
        operationEpoch.current += 1;
        activeSessionId.current = p.id;
        finishing.current = false;
        reviewActionPending.current = false;
        voiceActivity.current = initialVoiceActivityState();
        setSession(p);
        liveText.current = "";
        setVoice({
          voicePhase: "listening",
          partialText: "",
          committedText: "",
          notice: undefined,
        });
      }),
      onEvent<VoiceProcessStatus>("voice-process-status", (status) => {
        if (
          !shouldApplyVoiceProcessStatus(
            status.phase,
            status.sessionId,
            activeSessionId.current,
          )
        ) return;
        if (status.phase === "waking" && status.sessionId) {
          activeSessionId.current = status.sessionId;
          playRecordingCue("start", {
            enabled: settingsRef.current.sounds,
            sessionId: status.sessionId,
            inWindow: shouldPlayRecordingCueInWindow(isTauri(), isBubbleWindow),
          });
        }
        if (hideTimer.current) {
          window.clearTimeout(hideTimer.current);
          hideTimer.current = null;
        }
        setVoice({ voicePhase: status.phase, notice: status.message });
        if (status.phase === "failed") scheduleBubbleHide(BUBBLE_FAILURE_HIDE_MS);
      }),
      onEvent<{ sessionId: string }>("shortcut-stop-requested", (p) => {
        if (
          isBubbleWindow &&
          isEventForActiveSession(p.sessionId, activeSessionId.current)
        )
          void finish(p.sessionId);
      }),
      onEvent<{ sessionId: string }>("shortcut-cancel-requested", (p) => {
        if (
          isBubbleWindow &&
          isEventForActiveSession(p.sessionId, activeSessionId.current)
        )
          void cancel(p.sessionId);
      }),
      onEvent<{ sessionId: string; ai: boolean }>("shortcut-review-accept", (p) => {
        if (!isBubbleWindow) return;
        const currentReview = reviewRef.current;
        if (!currentReview || currentReview.sessionId !== p.sessionId) return;
        if (p.ai && !formattedTextRef.current) {
          void formatResult(currentReview);
          return;
        }
        const text = p.ai ? formattedTextRef.current : currentReview.text;
        if (p.ai && !text) return;
        void acceptReview(text, p.ai, currentReview);
      }),
      onEvent<{ sessionId: string }>("recording-stopped", (p) => {
        if (!isEventForActiveSession(p.sessionId, activeSessionId.current)) return;
        activeSessionId.current = null;
        reviewActionPending.current = false;
        setSession(null);
        setVoice({ audioLevel: 0, partialText: "" });
      }),
      onEvent<{ sessionId: string }>("recording-cancelled", (p) => {
        if (!isEventForActiveSession(p.sessionId, activeSessionId.current)) return;
        playRecordingCue("stop", {
          enabled: settingsRef.current.sounds,
          sessionId: p.sessionId,
          inWindow: shouldPlayRecordingCueInWindow(isTauri(), isBubbleWindow),
        });
        operationEpoch.current += 1;
        activeSessionId.current = null;
        reviewActionPending.current = false;
        setSession(null);
        setReview(null);
        setFormattedText("");
        setFormatting(false);
        setVoice({
          voicePhase: "idle",
          audioLevel: 0,
          partialText: "",
          committedText: "",
          notice: undefined,
        });
      }),
      onEvent<TranscriptSegment & { sessionId: string }>("asr-partial", (p) => {
        if (!isEventForActiveSession(p.sessionId, activeSessionId.current)) return;
        if (p.text.trim()) {
          markVoiceActivitySpeech(voiceActivity.current, performance.now());
        }
        liveText.current = p.text;
        setVoice({ partialText: p.text });
      }),
      onEvent<TranscriptSegment & { sessionId: string }>("asr-stable-segment", (p) => {
        if (!isEventForActiveSession(p.sessionId, activeSessionId.current)) return;
        if (p.text.trim()) {
          markVoiceActivitySpeech(voiceActivity.current, performance.now());
        }
        liveText.current = "";
        setVoice({ partialText: "", committedText: p.text });
      }),
      onEvent<{ sessionId: string; committedText: string; state: string; message?: string }>(
        "injection-state",
        (p) => {
          if (!isEventForActiveSession(p.sessionId, activeSessionId.current)) return;
          setVoice(p.state === "streaming"
            ? { notice: p.message }
            : { committedText: p.committedText, partialText: "", notice: p.message });
        },
      ),
      onEvent<AppError & { sessionId?: string }>("session-error", (p) => {
        if (
          p.sessionId &&
          !isEventForActiveSession(p.sessionId, activeSessionId.current)
        ) return;
        void writeDiagnosticLog("error", "ui.session_error_received", p.sessionId, {
          code: p.code,
          message: p.message,
          retryable: p.retryable,
          fallback: p.fallback,
        });
        if (p.sessionId) void recoverFailedSession(p.sessionId);
        setVoice({ voicePhase: "failed", audioLevel: 0, notice: p.message });
        scheduleBubbleHide(BUBBLE_FAILURE_HIDE_MS);
      }),
      onEvent<{ sessionId: string; message: string }>("session-warning", (p) => {
        if (!isEventForActiveSession(p.sessionId, activeSessionId.current)) return;
        setVoice({ notice: p.message });
      }),
    ];
    return () => {
      cleanups.forEach((p) => void p.then((fn) => fn()));
      if (hideTimer.current) window.clearTimeout(hideTimer.current);
    };
  }, []);
  useEffect(() => {
    if (!session?.startedAt) {
      setElapsedSeconds(0);
      return;
    }
    if (currentVoicePhase !== "listening") return;
    const update = () =>
      setElapsedSeconds(
        Math.max(0, (Date.now() - Date.parse(session.startedAt)) / 1000),
      );
    update();
    const timer = window.setInterval(update, 250);
    return () => window.clearInterval(timer);
  }, [session?.startedAt, currentVoicePhase]);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && session?.id) {
        event.preventDefault();
        void cancel(session.id);
      } else if (review && event.key === "Enter" && !event.ctrlKey) {
        event.preventDefault();
        void acceptReview(review.text, false);
      } else if (review && formattedText && event.key === "Enter" && event.ctrlKey) {
        event.preventDefault();
        void acceptReview(formattedText, true);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [session?.id, review, formattedText]);
  const begin = async () => {
    try {
      operationEpoch.current += 1;
      setVoice({ voicePhase: "waking", notice: undefined });
      await publishVoiceStatus({ phase: "waking" });
      let target: FocusTarget | null = null;
      if (isTauri() && !isBubbleWindow) {
        await getCurrentWindow().hide();
        await new Promise((resolve) => window.setTimeout(resolve, 180));
        target = await captureFocusTarget();
      }
      const current = await startRecording(target);
      void writeDiagnosticLog("info", "ui.recording_started", current.id, {
        bubbleWindow: isBubbleWindow,
        provider: settingsRef.current.asrProvider,
        model: settingsRef.current.localModelId || settingsRef.current.asrModel,
      });
      activeSessionId.current = current.id;
      finishing.current = false;
      voiceActivity.current = initialVoiceActivityState();
      liveText.current = "";
      setReview(null);
      setFormattedText("");
      setFormatting(false);
      setSession(current);
      setVoice({
        partialText: "",
        committedText: "",
        audioLevel: 0.1,
        voicePhase: "listening",
        notice: undefined,
      });
      if (!isTauri()) {
        playRecordingCue("start", {
          enabled: settingsRef.current.sounds,
          sessionId: current.id,
          inWindow: true,
        });
        let i = 0,
          committed = 0;
        demoTimer.current = window.setInterval(async () => {
          i = Math.min(demoSentence.length, i + 2);
          setVoice({
            partialText: demoSentence.slice(committed, i),
            audioLevel: 0.2 + Math.random() * 0.7,
          });
          if (i - committed >= 12 || i === demoSentence.length) {
            const text = demoSentence.slice(committed, i);
            committed = i;
            await injectSegment(current.id, {
              text,
              stable: true,
              startMs: 0,
              endMs: i * 80,
              confidence: 0.96,
            });
            setVoice({
              committedText: demoSentence.slice(0, committed),
              partialText: "",
            });
          }
          if (i === demoSentence.length && demoTimer.current) {
            clearInterval(demoTimer.current);
            demoTimer.current = null;
            void finish(current.id);
          }
        }, 120);
      }
    } catch (e) {
      const message = (e as AppError)?.message || String(e);
      void writeDiagnosticLog("error", "ui.recording_start_failed", undefined, {
        code: (e as AppError)?.code,
        message,
      });
      setVoice({ voicePhase: "failed", notice: message });
      await publishVoiceStatus({ phase: "failed", message });
    }
  };
  async function formatResult(current: RecordingReview) {
    const provider = settingsRef.current.llmProvider;
    const config = settingsRef.current.llm[provider];
    if (!settingsRef.current.llmEnabled || !config.configured || formatting) return;
    const epoch = ++operationEpoch.current;
    const isCurrent = () =>
      isCurrentVoiceOperation(
        epoch,
        operationEpoch.current,
        current.sessionId,
        activeSessionId.current,
      );
    setFormatting(true);
    setFormattedText("");
    setVoice({ notice: undefined, voicePhase: "transforming" });
    try {
      const output = await requestFormattedText(current);
      if (!isCurrent()) return;
      setFormattedText(output);
      setVoice({ voicePhase: "completed", notice: undefined });
    } catch (e) {
      if (!isCurrent()) return;
      setVoice({
        voicePhase: "completed",
        notice: (e as AppError)?.message || String(e),
      });
    } finally {
      if (isCurrent()) setFormatting(false);
    }
  }
  async function requestFormattedText(current: RecordingReview) {
    const provider = settingsRef.current.llmProvider;
    const config = settingsRef.current.llm[provider];
    if (!settingsRef.current.llmEnabled || !config.configured) {
      throw new Error("AI 格式化尚未配置");
    }
        const templates = await listPrompts();
    const selected = resolvePromptTemplate(
      templates,
      current.text,
      settingsRef.current.defaultPromptId,
    );
    const request = promptRequest(selected, current.text, current.hotwords);
    const output = await streamLlm({
      provider,
      model: config.model,
      baseUrl: config.baseUrl,
      instructions: request.instructions,
      input: request.input,
      maxOutputTokens: config.maxOutputTokens,
      thinking: config.thinking,
    });
    const text = output.trim();
    if (!text) throw new Error("AI 格式化未返回有效文字");
    return text;
  }
  const finish = async (id = session?.id) => {
    if (!id || finishing.current) return;
    if (isTauri() && !isBubbleWindow) {
      await requestVoiceStop(id);
      return;
    }
    finishing.current = true;
    const finishStartedAt = performance.now();
    const epoch = operationEpoch.current;
    const isCurrent = () =>
      isCurrentVoiceOperation(
        epoch,
        operationEpoch.current,
        id,
        activeSessionId.current,
      );
    if (demoTimer.current) {
      clearInterval(demoTimer.current);
      demoTimer.current = null;
    }
    try {
      void writeDiagnosticLog("info", "ui.finish_started", id, {
        bubbleWindow: isBubbleWindow,
        bubbleStyle: settingsRef.current.bubbleStyle,
        provider: settingsRef.current.asrProvider,
      });
      setVoice({ voicePhase: "recognizing", notice: undefined, audioLevel: 0 });
      await publishVoiceStatus({
        sessionId: id,
        phase: "recognizing",
      });
      let reviewTranscript: string | undefined;
      const streaming = usesStreamingAsr(settingsRef.current.asrProvider);
      if (isTauri() && isBubbleWindow && !streaming) {
        const transcriptionStartedAt = performance.now();
        void writeDiagnosticLog("info", "ui.final_transcription_started", id, {
          provider: settingsRef.current.asrProvider,
        });
        const segment = await transcribeFinal(id);
        void writeDiagnosticLog("info", "ui.final_transcription_completed", id, {
          elapsedMs: Math.round(performance.now() - transcriptionStartedAt),
          textLength: segment?.text?.length || 0,
        });
        if (!isCurrent()) return;
        if (segment?.text) {
          liveText.current = segment.text;
          reviewTranscript = segment.text;
          setVoice({ partialText: segment.text });
        }
      }
      if (!reviewTranscript?.trim()) {
        const current = useAppStore.getState();
        reviewTranscript = streamingTranscriptForReview(
          current.committedText,
          current.partialText || liveText.current,
        );
      }
      let result: RecordingReview;
      const reviewTimeoutMs = streaming ? 22_000 : 3_000;
      const reviewStartedAt = performance.now();
      try {
        result = await withReviewTimeout(
          stopRecordingForReview(
            id,
            reviewTranscript,
            !settingsRef.current.directInsert,
          ),
          reviewTimeoutMs,
        );
        playRecordingCue("stop", {
          enabled: settingsRef.current.sounds,
          sessionId: id,
          inWindow: shouldPlayRecordingCueInWindow(isTauri(), isBubbleWindow),
        });
        void writeDiagnosticLog("info", "ui.review_received", id, {
          elapsedMs: Math.round(performance.now() - reviewStartedAt),
          timeoutMs: reviewTimeoutMs,
          textLength: result.text.length,
        });
      } catch (error) {
        const current = useAppStore.getState();
        const recoveredText =
          reviewTranscript?.trim() ||
          liveText.current.trim() ||
          current.committedText.trim() ||
          current.partialText.trim();
        void writeDiagnosticLog("error", "ui.review_failed", id, {
          elapsedMs: Math.round(performance.now() - reviewStartedAt),
          timeoutMs: reviewTimeoutMs,
          code: (error as AppError)?.code,
          message: (error as AppError)?.message || String(error),
          recoveredTextLength: recoveredText.length,
        });
        if ((error as AppError)?.code !== "review.timeout" || !recoveredText) {
          throw error;
        }
        playRecordingCue("stop", {
          enabled: settingsRef.current.sounds,
          sessionId: id,
          inWindow: shouldPlayRecordingCueInWindow(isTauri(), isBubbleWindow),
        });
        result = recoveredRecordingReview({
          sessionId: id,
          text: recoveredText,
          duration: Math.floor(elapsedSeconds),
          engine: settingsRef.current.asrProvider,
          model:
            settingsRef.current.localModelId || settingsRef.current.asrModel,
        });
      }
      if (!isCurrent()) return;
      const completedReview = {
        ...result,
        duration: result.duration || Math.floor(elapsedSeconds),
      };
      if (!completedReview.text.trim()) {
        throw new Error("未识别到有效文字");
      }
      const outputFlow = voiceOutputFlow(settingsRef.current);
      if (outputFlow.direct) {
        let text = completedReview.text;
        let ai = false;
        let fallbackNotice: string | undefined;
        if (outputFlow.format) {
          setFormatting(true);
          setVoice({ voicePhase: "transforming", notice: undefined });
          try {
            text = await requestFormattedText(completedReview);
            ai = true;
          } catch {
            fallbackNotice = "AI 格式化失败，已上屏原文";
          } finally {
            setFormatting(false);
          }
          if (!isCurrent()) return;
        }
        await publishVoiceStatus({ sessionId: id, phase: "inserting" });
        await acceptReview(text, ai, completedReview, {
          hideImmediately: !fallbackNotice,
          successNotice: fallbackNotice,
        });
        return;
      }
      setReview(completedReview);
      setVoice({
        audioLevel: 0,
        partialText: "",
        committedText: completedReview.text,
        voicePhase: "completed",
        notice: undefined,
      });
      void writeDiagnosticLog("info", "ui.review_displayed", id, {
        elapsedMs: Math.round(performance.now() - finishStartedAt),
        textLength: completedReview.text.length,
      });
      if (
        settingsRef.current.llmEnabled &&
        outputFlow.format &&
        settingsRef.current.llm[settingsRef.current.llmProvider].configured
      ) {
        void formatResult(completedReview);
      }
      return;
    } catch (e) {
      if (!isCurrent()) return;
      const message = (e as AppError)?.message || String(e);
      void writeDiagnosticLog("error", "ui.finish_failed", id, {
        elapsedMs: Math.round(performance.now() - finishStartedAt),
        code: (e as AppError)?.code,
        message,
        provider: settingsRef.current.asrProvider,
      });
      try {
        await withReviewTimeout(stopRecording(id), 3_000);
      } catch {
        // The session may already have been stopped by another window.
      }
      playRecordingCue("stop", {
        enabled: settingsRef.current.sounds,
        sessionId: id,
        inWindow: shouldPlayRecordingCueInWindow(isTauri(), isBubbleWindow),
      });
      setSession(null);
      setVoice({ voicePhase: "failed", audioLevel: 0, notice: message });
      scheduleBubbleHide(BUBBLE_FAILURE_HIDE_MS);
      await publishVoiceStatus({
        sessionId: id,
        phase: "failed",
        message,
      });
      if (activeSessionId.current === id) activeSessionId.current = null;
    } finally {
      if (operationEpoch.current === epoch) finishing.current = false;
    }
  };
  const acceptReview = async (
    text: string,
    ai: boolean,
    currentReview = reviewRef.current,
    options: { hideImmediately?: boolean; successNotice?: string } = {},
  ) => {
    if (!currentReview || reviewActionPending.current) return;
    reviewActionPending.current = true;
    operationEpoch.current += 1;
    setFormatting(false);
    const provider = settingsRef.current.llmProvider;
    try {
      const result = (await commitRecordingReview({
        sessionId: currentReview.sessionId,
        text,
        ai,
        llmProvider: ai ? provider : undefined,
        llmModel: ai ? settingsRef.current.llm[provider].model : undefined,
      })) as { state?: string; message?: string } | undefined;
      if (result?.state === "focus_lost") {
        setReview(currentReview);
        setVoice({
          voicePhase: "completed",
          notice: result.message || "焦点已离开原输入框",
        });
        return;
      }
      activeSessionId.current = null;
      setSession(null);
      setReview(null);
      setFormattedText("");
      if (options.successNotice) {
        setVoice({ voicePhase: "completed", notice: options.successNotice });
        scheduleBubbleHide(1800);
      } else if (options.hideImmediately) {
        scheduleBubbleHide(0);
      } else {
        setVoice({ voicePhase: "completed", notice: ai ? "已采纳并插入" : "已插入上屏" });
        scheduleBubbleHide(500);
      }
    } catch (e) {
      setReview(currentReview);
      setVoice({
        voicePhase: "completed",
        notice: (e as AppError)?.message || String(e),
      });
    } finally {
      reviewActionPending.current = false;
    }
  };
  const copyReview = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setVoice({ notice: "已复制到剪贴板" });
    } catch {
      setVoice({ notice: "复制失败，请检查剪贴板权限" });
    }
  };
  const discardReview = async () => {
    if (!review || reviewActionPending.current) return;
    reviewActionPending.current = true;
    operationEpoch.current += 1;
    setFormatting(false);
    try {
      await cancelRecording(review.sessionId);
      activeSessionId.current = null;
      setSession(null);
      setReview(null);
      setFormattedText("");
      scheduleBubbleHide(0);
    } catch (e) {
      setVoice({ notice: (e as AppError)?.message || String(e) });
    } finally {
      reviewActionPending.current = false;
    }
  };
  const cancel = async (id = session?.id) => {
    operationEpoch.current += 1;
    finishing.current = false;
    setFormatting(false);
    try {
      if (id) await cancelRecording(id);
    } catch (e) {
      setVoice({ voicePhase: "failed", notice: (e as AppError)?.message || String(e) });
      return;
    }
    if (demoTimer.current) clearInterval(demoTimer.current);
    if (!isTauri() && id) {
      playRecordingCue("stop", {
        enabled: settingsRef.current.sounds,
        sessionId: id,
        inWindow: true,
      });
    }
    activeSessionId.current = null;
    setSession(null);
    setReview(null);
    setFormattedText("");
    setVoice({
      voicePhase: "idle",
      audioLevel: 0,
      partialText: "",
      committedText: "",
      notice: undefined,
    });
  };
  return {
    begin,
    finish,
    cancel,
    review,
    formattedText,
    formatting,
    elapsedSeconds,
    formatReview: () => review && formatResult(review),
    acceptReview,
    keepOriginal: () => {
      setFormattedText("");
      setVoice({ voicePhase: "completed", notice: undefined });
    },
    copyReview,
    discardReview,
    dismissFailure: () => {
      if (hideTimer.current) {
        window.clearTimeout(hideTimer.current);
        hideTimer.current = null;
      }
      scheduleBubbleHide(0);
    },
  };
}
