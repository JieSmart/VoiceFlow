import { useLayoutEffect, useRef } from "react";
import {
  Check,
  Copy,
  Microphone,
  Sparkle,
  Trash,
  WarningCircle,
} from "@phosphor-icons/react";
import clsx from "clsx";
import { formatWakeHint } from "../hotkeys";
import { usesStreamingAsr } from "../voiceReview";
import { bubbleAStage, voiceProcessCopy } from "../voiceProcess";
import { useAppStore } from "../store";
import type { AppSettings, VoiceProcessStatus } from "../types";
import { formatRecordingTime } from "../ui/primitives";
import { ProcessingSparkle } from "./ProcessingSparkle";
import type { BubbleInteraction, VoiceSession } from "../voice/types";

type BubbleVariantProps = {
  voice: VoiceSession;
  interaction: BubbleInteraction;
  dragInteraction: BubbleInteraction;
  text: string;
  level: number;
  phase: VoiceProcessStatus["phase"];
  notice?: string;
};

function useWakeHint() {
  return formatWakeHint(useAppStore((state) => state.settings.hotkeys.wake));
}

function useProcessCopy(phase: VoiceProcessStatus["phase"], hasTranscript: boolean) {
  const settings = useAppStore((state) => state.settings);
  return voiceProcessCopy(phase, hasTranscript, {
    streaming: usesStreamingAsr(settings.asrProvider),
    wake: settings.hotkeys.wake,
  });
}

function HighlightedTranscript({
  text,
  matched,
}: {
  text: string;
  matched: string[];
}) {
  const configured = useAppStore((s) => s.hotwords);
  const candidates = Array.from(
    new Set(
      matched.flatMap((word) => {
        const item = configured.find((hotword) => hotword.word === word);
        return [word, item?.replacement].filter(Boolean) as string[];
      }),
    ),
  ).sort((a, b) => b.length - a.length);
  if (!candidates.length) return <>{text}</>;
  const nodes: React.ReactNode[] = [];
  let cursor = 0;
  while (cursor < text.length) {
    let nextIndex = -1;
    let nextWord = "";
    for (const word of candidates) {
      const index = text.indexOf(word, cursor);
      if (index >= 0 && (nextIndex < 0 || index < nextIndex)) {
        nextIndex = index;
        nextWord = word;
      }
    }
    if (nextIndex < 0) {
      nodes.push(text.slice(cursor));
      break;
    }
    if (nextIndex > cursor) nodes.push(text.slice(cursor, nextIndex));
    nodes.push(
      <mark className="bubble-a-hotword" key={`${nextIndex}-${nextWord}`}>
        {nextWord}
      </mark>,
    );
    cursor = nextIndex + nextWord.length;
  }
  return <>{nodes}</>;
}

function BubbleReview({
  voice,
  dragInteraction,
  notice,
  style,
}: {
  voice: VoiceSession;
  dragInteraction: BubbleInteraction;
  notice?: string;
  style: AppSettings["bubbleStyle"];
}) {
  const settings = useAppStore((s) => s.settings);
  const review = voice.review;
  if (!review) return null;
  const provider = settings.llmProvider;
  const llm = settings.llm[provider];
  const formatted = voice.formattedText;
  const contents = (
    <>
      <header className="bubble-a-card-head" {...dragInteraction}>
        <div>
          <span className="bubble-a-status"><Check size={13} weight="bold" />识别完成</span>
          <span className="bubble-a-engine">
            {review.engine}{review.model ? ` · ${review.model.split(/[\\/]/).pop()}` : ""}
          </span>
        </div>
        <span>{formatRecordingTime(review.duration)} · {review.text.length} 字</span>
      </header>
      {voice.formatting ? (
        <div className="bubble-a-formatting" role="status">
          <ProcessingSparkle />
          {llm.model} 正在整理文本…
        </div>
      ) : formatted ? (
        <div className="bubble-a-formatted">
          <div className="bubble-a-ai-label"><Sparkle size={13} />{llm.model} · AI 格式化</div>
          <div>{formatted}</div>
        </div>
      ) : (
        <div className="bubble-a-transcript">
          <HighlightedTranscript text={review.text} matched={review.hotwords} />
        </div>
      )}
      {notice && <div className="bubble-a-notice" role="status">{notice}</div>}
      <div className="bubble-a-actions">
        {formatted ? (
          <>
            <button className="button primary" onClick={() => void voice.acceptReview(formatted, true)}>
              <Check />采纳并插入
            </button>
            <button className="button" onClick={voice.keepOriginal}>保留原文</button>
            <button className="button ghost" onClick={() => void voice.copyReview(formatted)}><Copy />复制</button>
            <button className="button ghost danger" onClick={() => void voice.discardReview()}><Trash />丢弃</button>
          </>
        ) : (
          <>
            <button className="button primary" onClick={() => void voice.acceptReview(review.text, false)}>
              <Check />插入上屏
            </button>
            <button
              className="button"
              disabled={!settings.llmEnabled || !llm.configured || voice.formatting}
              title={
                !settings.llmEnabled
                  ? "请先在设置 → 大模型中启用 AI 格式化"
                  : llm.configured
                    ? ""
                    : "请先在设置 → 大模型中完成配置"
              }
              onClick={() => void voice.formatReview()}
            ><Sparkle />AI 格式化</button>
            <button className="button ghost" onClick={() => void voice.copyReview(review.text)}><Copy />复制</button>
            <button className="button ghost danger" onClick={() => void voice.discardReview()}><Trash />丢弃</button>
          </>
        )}
      </div>
      <footer className="bubble-a-card-foot">
        <span>尚未上屏；确认后写入原输入框</span>
        <span>Enter 确认 · Ctrl+Enter 采纳 · Esc 丢弃</span>
      </footer>
    </>
  );

  if (style === "C") {
    return (
      <div className="bubble-c-anchor">
        <section className="bubble-c-bubble review bubble-review-card" aria-label="语音识别结果确认">
          <div className="bubble-c-wave settled" aria-hidden="true">
            {[8, 16, 26, 14, 20].map((height, index) => <span key={index} style={{ height }} />)}
          </div>
          <div className="bubble-c-content">{contents}</div>
        </section>
      </div>
    );
  }

  return (
    <section
      className={clsx(
        "bubble-review-card",
        style === "A" ? "bubble-a-card" : "bubble-b-card bubble-b-review",
      )}
      aria-label="语音识别结果确认"
    >
      {contents}
    </section>
  );
}

export function BubbleA({
  voice,
  interaction,
  dragInteraction,
  text,
  level,
  phase,
  notice,
}: BubbleVariantProps) {
  const review = voice.review;
  const wakeHint = useWakeHint();
  const [transitionTitle, transitionDetail] = useProcessCopy(phase, Boolean(text));
  const stage = bubbleAStage(phase, Boolean(review));
  const liveTextRef = useRef<HTMLDivElement>(null);
  const bars = Array.from({ length: 7 }, (_, index) =>
    Math.max(3, Math.min(16, 3 + level * (7 + ((index * 5) % 4) * 3))),
  );
  useLayoutEffect(() => {
    const element = liveTextRef.current;
    if (!element) return;
    element.scrollTop = text ? element.scrollHeight : 0;
  }, [text]);
  if (!review) {
    if (phase === "failed") {
      return (
        <div className="bubble-a-wrap bubble-a-failed-wrap">
          <div
            className="bubble-a-failure-card"
            {...interaction}
            aria-label="关闭失败提示"
          >
            <WarningCircle className="bubble-a-failure-icon" weight="fill" />
            <div className="bubble-a-failure-copy" role="status">
              <strong>处理失败</strong>
              <small>{notice || "这次语音没有生成可用文字"}</small>
            </div>
          </div>
          <div className="bubble-a-failure-hint">点击关闭 · 按 {wakeHint} 重试</div>
        </div>
      );
    }
    const listening = stage === "recording";
    return (
      <div className="bubble-a-wrap">
        <div
          className={clsx(
            "bubble-a-capsule",
            listening ? "recording breathing" : "processing",
          )}
          {...interaction}
        >
          {listening ? (
            <>
              <i className="bubble-a-recording-dot" />
              <div className="bubble-a-wave" aria-hidden="true">
                {bars.map((height, index) => (
                  <span key={index} style={{ height }} />
                ))}
              </div>
              <div
                ref={liveTextRef}
                className={clsx("bubble-a-live", !text && "placeholder")}
              >
                {text || "开始说话，转写将在这里流式出现…"}
              </div>
              <time>{formatRecordingTime(voice.elapsedSeconds)}</time>
            </>
          ) : (
            <>
              <ProcessingSparkle />
              <div className="bubble-a-processing-copy" role="status">
                <strong>{transitionTitle}</strong>
                <small>{transitionDetail}</small>
              </div>
            </>
          )}
        </div>
        {notice && <div className="bubble-a-notice" role="status">{notice}</div>}
        <div className="bubble-a-hint">
          {listening && `再次按 ${wakeHint} 或点击胶囊结束`}
          <span>Esc 丢弃</span>
        </div>
      </div>
    );
  }
  return <BubbleReview voice={voice} dragInteraction={dragInteraction} notice={notice} style="A" />;
}

export function BubbleB({
  voice,
  interaction,
  dragInteraction,
  text,
  level,
  phase,
  notice,
}: BubbleVariantProps) {
  const review = voice.review;
  const wakeHint = useWakeHint();
  const listening = phase === "listening" && !review;
  const [statusTitle, statusDetail] = useProcessCopy(phase, Boolean(text));
  const liveTextRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const element = liveTextRef.current;
    if (!element) return;
    element.scrollTop = text ? element.scrollHeight : 0;
  }, [text]);

  if (review) {
    return <BubbleReview voice={voice} dragInteraction={dragInteraction} notice={notice} style="B" />;
  }
  if (phase === "failed") {
    return (
      <div className="bubble-b-card bubble-b-failed" {...interaction}>
        <WarningCircle className="bubble-b-failure-icon" weight="fill" />
        <div role="status">
          <strong>处理失败</strong>
          <small>{notice || "这次语音没有生成可用文字"}</small>
          <span>点击关闭 · 按 {wakeHint} 重试</span>
        </div>
      </div>
    );
  }

  return (
    <div className={clsx("bubble-b-card", listening ? "recording" : "processing")} {...interaction}>
      <div className="bubble-b-orb-stage" aria-hidden="true">
        {[0, 1, 2].map((index) => (
          <span
            key={index}
            className={clsx("bubble-b-ring", `ring-${index + 1}`, listening && "level-driven")}
            style={listening ? {
              transform: `scale(${1 + level * (0.45 - index * 0.12)})`,
              opacity: Math.max(0, Math.min(0.9, level * (1.6 - index * 0.45) - 0.08)),
            } : undefined}
          />
        ))}
        <div
          className="bubble-b-orb"
          style={listening ? { transform: `scale(${1 + level * 0.06})` } : undefined}
        >
          {listening ? <Microphone size={32} weight="fill" /> : <ProcessingSparkle size={28} />}
        </div>
      </div>
      {listening ? (
        <>
          <div className="bubble-b-timer">{formatRecordingTime(voice.elapsedSeconds)}</div>
          <div className="bubble-b-listening">正在聆听 · 再按 {wakeHint} 结束</div>
          <div ref={liveTextRef} className={clsx("bubble-b-live", !text && "placeholder")}>
            {text || "开始说话，转写将在这里流式出现…"}
          </div>
          <div className="bubble-b-hint">点击卡片结束 · Esc 丢弃</div>
        </>
      ) : (
        <div className="bubble-b-processing-copy" role="status">
          <strong>{statusTitle}</strong>
          <small>{statusDetail}</small>
          {notice && <span>{notice}</span>}
        </div>
      )}
    </div>
  );
}

export function BubbleC({
  voice,
  interaction,
  dragInteraction,
  text,
  level,
  phase,
  notice,
}: BubbleVariantProps) {
  const review = voice.review;
  const wakeHint = useWakeHint();
  const listening = phase === "listening" && !review;
  const [statusTitle, statusDetail] = useProcessCopy(phase, Boolean(text));
  const bars = Array.from({ length: 5 }, (_, index) => {
    const profile = 0.45 + 0.55 * Math.sin((Math.PI * (index + 1)) / 6);
    return Math.max(3, Math.min(30, 3 + level * profile * 27));
  });

  if (review) {
    return <BubbleReview voice={voice} dragInteraction={dragInteraction} notice={notice} style="C" />;
  }

  if (phase === "failed") {
    return (
      <div className="bubble-c-anchor">
        <div className="bubble-c-bubble failed" {...interaction}>
          <WarningCircle className="bubble-c-failure-icon" weight="fill" />
          <div className="bubble-c-failure-copy" role="status">
            <strong>处理失败</strong>
            <small>{notice || "这次语音没有生成可用文字"}</small>
            <span>点击关闭 · 按 {wakeHint} 重试</span>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="bubble-c-anchor">
      <div className={clsx("bubble-c-bubble", listening ? "recording floating" : "processing")} {...interaction}>
        <div className={clsx("bubble-c-wave", !listening && "processing")} aria-hidden="true">
          {bars.map((height, index) => <span key={index} style={{ height }} />)}
        </div>
        <div className="bubble-c-content">
          {listening ? (
            <>
              <div className="bubble-c-status">
                <i />
                <strong>正在聆听</strong>
                <time>{formatRecordingTime(voice.elapsedSeconds)}</time>
              </div>
              <div className={clsx("bubble-c-text", !text && "placeholder")}>
                {text || "开始说话，转写将在这里流式出现…"}
                <span className="bubble-c-caret" aria-hidden="true" />
              </div>
            </>
          ) : (
            <div className="bubble-c-processing-copy" role="status">
              <ProcessingSparkle />
              <div>
                <strong>{statusTitle}</strong>
                <small>{statusDetail}</small>
                {notice && <span>{notice}</span>}
              </div>
            </div>
          )}
        </div>
      </div>
      <div className="bubble-c-hint">
        {listening ? `再次按 ${wakeHint} 或点击气泡结束 · Esc 丢弃` : "Esc 丢弃"}
      </div>
    </div>
  );
}
