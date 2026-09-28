import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ComponentProps } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BubbleApp } from "./bubble/BubbleApp";
import { VoiceBubble } from "./bubble/VoiceBubble";
import { BubbleA, BubbleB, BubbleC } from "./bubble/variants";
import { defaultSettings, useAppStore } from "./store";
import type { RecordingReview, RecordingSession } from "./types";

const tauriWindowMocks = vi.hoisted(() => ({
  getCurrentWindow: vi.fn(),
}));

vi.mock("@tauri-apps/api/window", async (importOriginal) => ({
  ...await importOriginal<typeof import("@tauri-apps/api/window")>(),
  getCurrentWindow: tauriWindowMocks.getCurrentWindow,
}));

type Voice = ComponentProps<typeof BubbleA>["voice"];

const session: RecordingSession = {
  id: "session-a",
  state: "capturing",
  target: {
    windowId: 42,
    processId: 7,
    title: "测试输入框",
    capturedAt: "2026-09-07T00:00:00Z",
  },
  startedAt: "2026-09-07T00:00:00Z",
  committedText: "",
};

const review: RecordingReview = {
  sessionId: "session-a",
  text: "语流已经完成识别",
  duration: 12,
  hotwords: ["语流"],
  engine: "sherpa-streaming",
  model: "models/sherpa-zh-en",
};

function voice(overrides: Partial<Voice> = {}): Voice {
  return {
    begin: vi.fn(),
    finish: vi.fn(),
    cancel: vi.fn(),
    review: null,
    formattedText: "",
    formatting: false,
    elapsedSeconds: 0,
    formatReview: vi.fn(),
    acceptReview: vi.fn(),
    keepOriginal: vi.fn(),
    copyReview: vi.fn(),
    discardReview: vi.fn(),
    dismissFailure: vi.fn(),
    ...overrides,
  } as Voice;
}

function renderBubbleA(
  currentVoice: Voice,
  options: {
    text?: string;
    level?: number;
    phase?: ComponentProps<typeof BubbleA>["phase"];
    notice?: string;
  } = {},
) {
  return render(
    <BubbleA
      voice={currentVoice}
      interaction={{}}
      dragInteraction={{}}
      text={options.text ?? ""}
      level={options.level ?? 0}
      phase={options.phase ?? "listening"}
      notice={options.notice}
    />,
  );
}

describe("方案 A 语音气泡完整视觉流程", () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    tauriWindowMocks.getCurrentWindow.mockReset();
    vi.unstubAllGlobals();
  });

  beforeEach(() => {
    useAppStore.setState({
      session: null,
      partialText: "",
      committedText: "",
      audioLevel: 0,
      voicePhase: "idle",
      notice: undefined,
      settings: defaultSettings,
    });
  });

  it("待命阶段显示启动入口，点击后开始录音", () => {
    const currentVoice = voice();
    render(<VoiceBubble voice={currentVoice} />);

    const trigger = screen.getByRole("button", { name: "开始录音" });
    expect(trigger).toHaveTextContent("语流已待命");
    expect(trigger).toHaveTextContent("按 Ctrl + Shift + Space 开始");
    fireEvent.pointerDown(trigger, { button: 0, clientX: 10, clientY: 10, pointerId: 1 });
    fireEvent.pointerUp(trigger, { button: 0, clientX: 10, clientY: 10, pointerId: 1 });
    expect(currentVoice.begin).toHaveBeenCalledTimes(1);
  });

  it("浏览器预览不等待原生窗口动画并立即展示新状态", () => {
    render(<VoiceBubble voice={voice()} />);
    expect(screen.getByText("语流已待命")).toBeInTheDocument();

    act(() => {
      useAppStore.setState({ session, voicePhase: "listening" });
    });

    expect(screen.getByRole("button", { name: "结束录音" })).toBeInTheDocument();
    expect(screen.getByText("开始说话，转写将在这里流式出现…"))
      .toBeInTheDocument();
  });

  it("首次激活先设置内部尺寸和位置，再显示并呈现新内容", async () => {
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true })));
    let currentSize = { width: 390, height: 78 };
    let finishResize: (() => void) | undefined;
    const setPosition = vi.fn().mockResolvedValue(undefined);
    const show = vi.fn().mockResolvedValue(undefined);
    const setSize = vi.fn((size: { width: number; height: number }) => (
      new Promise<void>((resolve) => {
        finishResize = () => {
          currentSize = { width: size.width, height: size.height };
          resolve();
        };
      })
    ));
    tauriWindowMocks.getCurrentWindow.mockReturnValue({
      scaleFactor: vi.fn().mockResolvedValue(1),
      outerPosition: vi.fn().mockResolvedValue({ x: 100, y: 100 }),
      outerSize: vi.fn(() => Promise.resolve(currentSize)),
      innerSize: vi.fn(() => Promise.resolve(currentSize)),
      setSize,
      setPosition,
      show,
    } as never);

    render(<VoiceBubble voice={voice()} />);
    await act(async () => { await Promise.resolve(); });
    expect(screen.getByText("语流已待命")).toBeInTheDocument();

    act(() => {
      useAppStore.setState({ session, voicePhase: "listening" });
    });
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(setSize).toHaveBeenCalled();
    expect(screen.getByText("语流已待命")).toBeInTheDocument();
    expect(screen.queryByText("开始说话，转写将在这里流式出现…"))
      .not.toBeInTheDocument();

    await act(async () => {
      finishResize?.();
      await Promise.resolve();
      await Promise.resolve();
    });
    await waitFor(() => {
      expect(setPosition).toHaveBeenCalledWith(expect.objectContaining({
        x: -15,
        y: 62,
      }));
      expect(show).toHaveBeenCalledTimes(1);
      expect(screen.getByText("开始说话，转写将在这里流式出现…"))
        .toBeInTheDocument();
    });
    expect(setPosition.mock.invocationCallOrder[0])
      .toBeLessThan(show.mock.invocationCallOrder[0]);
  });

  it.each(["B", "C"] as const)(
    "气泡样式 %s 在审核模式下仍展示统一结果卡",
    (bubbleStyle) => {
      useAppStore.setState({
        session,
        committedText: review.text,
        voicePhase: "completed",
        settings: { ...defaultSettings, bubbleStyle, directInsert: false },
      });
      render(<VoiceBubble voice={voice({ review })} />);

      expect(screen.getByLabelText("语音识别结果确认")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /插入上屏/ })).toBeInTheDocument();
    },
  );

  it("右键阻止系统菜单且不弹出气泡菜单", () => {
    const { container } = render(<BubbleApp />);
    const bubbleWindow = container.querySelector(".bubble-window");
    expect(bubbleWindow).not.toBeNull();

    const nativeMenuAllowed = fireEvent.contextMenu(bubbleWindow!, {
      clientX: 30,
      clientY: 30,
    });

    expect(nativeMenuAllowed).toBe(false);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "重启语流" })).not.toBeInTheDocument();
  });

  it("唤醒阶段显示麦克风连接反馈和丢弃提示", () => {
    const { container } = renderBubbleA(voice(), { phase: "waking" });

    expect(screen.getByRole("status")).toHaveTextContent("正在唤醒");
    expect(screen.getByRole("status")).toHaveTextContent("正在连接麦克风");
    expect(screen.getByText("Esc 丢弃")).toBeInTheDocument();
    expect(container.querySelector(".processing-sparkle")).toBeInTheDocument();
  });

  it("录音阶段显示录音点、动态音量柱、实时文本和时长", () => {
    const { container, rerender } = renderBubbleA(
      voice({ elapsedSeconds: 65 }),
      { phase: "listening", level: 0 },
    );

    expect(container.querySelector(".bubble-a-recording-dot")).toBeInTheDocument();
    expect(container.querySelectorAll(".bubble-a-wave span")).toHaveLength(7);
    expect(screen.getByText("开始说话，转写将在这里流式出现…")).toBeInTheDocument();
    expect(screen.getByText("1:05")).toBeInTheDocument();
    expect(screen.getByText(/再次按 Ctrl \+ Shift \+ Space/)).toBeInTheDocument();
    const quietHeight = container.querySelector<HTMLElement>(".bubble-a-wave span")?.style.height;

    rerender(
      <BubbleA
        voice={voice({ elapsedSeconds: 65 })}
        interaction={{}}
        dragInteraction={{}}
        text="正在实时转写"
        level={1}
        phase="listening"
      />,
    );
    expect(screen.getByText("正在实时转写")).toBeInTheDocument();
    expect(container.querySelector<HTMLElement>(".bubble-a-wave span")?.style.height)
      .not.toBe(quietHeight);
  });

  it("长文本流式更新时保留全文、跟随最新三行并在清空后复位", () => {
    const initialText = "第一段较短的实时转写";
    const latestText = `${initialText}，继续补充第二行、第三行，以及应该顶出第一行的第四行内容。`;
    const view = renderBubbleA(voice(), { phase: "listening", text: initialText });
    const liveText = view.container.querySelector<HTMLElement>(".bubble-a-live");
    expect(liveText).not.toBeNull();

    Object.defineProperty(liveText!, "scrollHeight", {
      configurable: true,
      value: 96,
    });
    liveText!.scrollTop = 0;

    view.rerender(
      <BubbleA
        voice={voice()}
        interaction={{}}
        dragInteraction={{}}
        text={latestText}
        level={0}
        phase="listening"
      />,
    );

    expect(liveText).toHaveTextContent(latestText);
    expect(liveText!.scrollTop).toBe(96);

    view.rerender(
      <BubbleA
        voice={voice()}
        interaction={{}}
        dragInteraction={{}}
        text=""
        level={0}
        phase="listening"
      />,
    );

    expect(liveText!.scrollTop).toBe(0);
    expect(liveText).toHaveTextContent("开始说话，转写将在这里流式出现…");
  });

  it("点击录音胶囊会触发停止", () => {
    const currentVoice = voice();
    useAppStore.setState({ session, voicePhase: "listening" });
    render(<VoiceBubble voice={currentVoice} />);

    const trigger = screen.getByRole("button", { name: "结束录音" });
    expect(trigger).toHaveAttribute("tabindex", "-1");
    fireEvent.pointerDown(trigger, { button: 0, clientX: 10, clientY: 10, pointerId: 2 });
    fireEvent.pointerUp(trigger, { button: 0, clientX: 10, clientY: 10, pointerId: 2 });
    expect(currentVoice.finish).toHaveBeenCalledTimes(1);
  });

  it.each([
    ["recognizing", "正在识别", "正在等待最终结果"],
    ["transforming", "正在整理", "正在处理识别结果"],
    ["inserting", "正在写入", "正在写回原输入框"],
  ] as const)("%s 阶段显示对应的处理中反馈", (phase, title, detail) => {
    renderBubbleA(voice(), { phase, text: "已有转写" });
    expect(screen.getByRole("status")).toHaveTextContent(title);
    expect(screen.getByRole("status")).toHaveTextContent(detail);
    expect(document.querySelector(".processing-sparkle")).not.toBeNull();
    expect(document.querySelector(".processing-sparkle-lottie")).not.toBeNull();
  });

  it("结果阶段展示识别信息、热词高亮和四个操作入口", () => {
    const currentVoice = voice({ review });
    const { container } = renderBubbleA(currentVoice, { phase: "completed" });

    expect(screen.getByText("识别完成")).toBeInTheDocument();
    expect(container.querySelector(".bubble-a-engine")).toHaveTextContent(
      "sherpa-streaming · sherpa-zh-en",
    );
    expect(screen.getByText("0:12 · 8 字")).toBeInTheDocument();
    expect(screen.getByText("语流")).toHaveClass("bubble-a-hotword");
    expect(screen.getByRole("button", { name: /插入上屏/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /AI 格式化/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /复制/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /丢弃/ })).toBeInTheDocument();
    expect(container.querySelector(".bubble-a-card")).toBeInTheDocument();
    expect(screen.getByText(/尚未上屏；确认后写入原输入框/)).toBeInTheDocument();
  });

  it("结果卡操作调用对应的确认、复制和丢弃功能", () => {
    const currentVoice = voice({ review });
    renderBubbleA(currentVoice, { phase: "completed" });

    fireEvent.click(screen.getByRole("button", { name: /插入上屏/ }));
    fireEvent.click(screen.getByRole("button", { name: /复制/ }));
    fireEvent.click(screen.getByRole("button", { name: /丢弃/ }));
    expect(currentVoice.acceptReview).toHaveBeenCalledWith(review.text, false);
    expect(currentVoice.copyReview).toHaveBeenCalledWith(review.text);
    expect(currentVoice.discardReview).toHaveBeenCalledTimes(1);
  });

  it("关闭 AI 格式化总开关后禁用格式化入口", () => {
    useAppStore.setState({
      settings: {
        ...defaultSettings,
        llmEnabled: false,
        llm: {
          ...defaultSettings.llm,
          openai: { ...defaultSettings.llm.openai, configured: true },
        },
      },
    });
    renderBubbleA(voice({ review }), { phase: "completed" });

    expect(screen.getByRole("button", { name: /AI 格式化/ })).toBeDisabled();
  });

  it("AI 整理中和整理完成分别显示加载反馈及采纳入口", () => {
    useAppStore.setState({
      settings: {
        ...defaultSettings,
        llm: {
          ...defaultSettings.llm,
          openai: { ...defaultSettings.llm.openai, configured: true },
        },
      },
    });
    const loading = voice({ review, formatting: true });
    const view = renderBubbleA(loading, { phase: "transforming" });
    expect(screen.getByRole("status")).toHaveTextContent("gpt-5-mini 正在整理文本…");

    const completed = voice({ review, formattedText: "整理后的语流文本" });
    view.rerender(
      <BubbleA
        voice={completed}
        interaction={{}}
        dragInteraction={{}}
        text=""
        level={0}
        phase="completed"
      />,
    );
    expect(screen.getByText("整理后的语流文本")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /采纳并插入/ }));
    fireEvent.click(screen.getByRole("button", { name: "保留原文" }));
    expect(completed.acceptReview).toHaveBeenCalledWith("整理后的语流文本", true);
    expect(completed.keepOriginal).toHaveBeenCalledTimes(1);
  });

  it("提交成功和失败都显示明确的最终反馈", () => {
    const view = renderBubbleA(voice(), { phase: "completed" });
    expect(screen.getByRole("status")).toHaveTextContent("已输入");
    expect(screen.getByRole("status")).toHaveTextContent("文字已写入");

    view.rerender(
      <BubbleA
        voice={voice()}
        interaction={{}}
        dragInteraction={{}}
        text=""
        level={0}
        phase="failed"
        notice="麦克风不可用"
      />,
    );
    expect(screen.getByText("处理失败")).toBeInTheDocument();
    expect(screen.getByText("麦克风不可用")).toBeInTheDocument();
    expect(screen.queryByText("Esc 丢弃")).not.toBeInTheDocument();
    expect(screen.getByText("点击关闭 · 按 Ctrl + Shift + Space 重试")).toBeInTheDocument();
  });

  it("方案 B 使用真实音量驱动三层圆环、麦克风和录音计时", () => {
    const { container } = render(
      <BubbleB
        voice={voice({ elapsedSeconds: 65 })}
        interaction={{}}
        dragInteraction={{}}
        text="正在实时转写"
        level={0.8}
        phase="listening"
      />,
    );

    expect(container.querySelector(".bubble-b-card.recording")).toBeInTheDocument();
    expect(container.querySelectorAll(".bubble-b-ring.level-driven")).toHaveLength(3);
    expect(container.querySelector<HTMLElement>(".bubble-b-ring")?.style.transform)
      .not.toBe("scale(1)");
    expect(container.querySelector(".bubble-b-orb svg")).toBeInTheDocument();
    expect(screen.getByText("1:05")).toBeInTheDocument();
    expect(screen.getByText("正在实时转写")).toBeInTheDocument();
  });

  it("方案 B 在处理和审核阶段保留自己的卡片外壳", () => {
    const view = render(
      <BubbleB
        voice={voice()}
        interaction={{}}
        dragInteraction={{}}
        text="已有转写"
        level={0}
        phase="recognizing"
      />,
    );
    expect(view.container.querySelector(".bubble-b-card.processing")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("正在识别");

    view.rerender(
      <BubbleB
        voice={voice({ review })}
        interaction={{}}
        dragInteraction={{}}
        text={review.text}
        level={0}
        phase="completed"
      />,
    );
    expect(view.container.querySelector(".bubble-b-review")).toBeInTheDocument();
    expect(screen.getByLabelText("语音识别结果确认")).toBeInTheDocument();
  });

  it("方案 C 展示音量 VU、悬浮气泡、计时和打字光标", () => {
    const { container } = render(
      <BubbleC
        voice={voice({ elapsedSeconds: 8 })}
        interaction={{}}
        dragInteraction={{}}
        text="墨迹气泡实时文字"
        level={0.9}
        phase="listening"
      />,
    );

    expect(container.querySelector(".bubble-c-bubble.floating")).toBeInTheDocument();
    expect(container.querySelectorAll(".bubble-c-wave span")).toHaveLength(5);
    expect(container.querySelector<HTMLElement>(".bubble-c-wave span")?.style.height)
      .not.toBe("3px");
    expect(container.querySelector(".bubble-c-caret")).toBeInTheDocument();
    expect(screen.getByText("0:08")).toBeInTheDocument();
  });

  it("方案 C 在处理、失败和审核阶段保持墨迹气泡结构", () => {
    const view = render(
      <BubbleC
        voice={voice()}
        interaction={{}}
        dragInteraction={{}}
        text=""
        level={0}
        phase="transforming"
      />,
    );
    expect(view.container.querySelector(".bubble-c-bubble.processing")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("正在整理");

    view.rerender(
      <BubbleC
        voice={voice()}
        interaction={{}}
        dragInteraction={{}}
        text=""
        level={0}
        phase="failed"
        notice="麦克风不可用"
      />,
    );
    expect(view.container.querySelector(".bubble-c-bubble.failed")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("麦克风不可用");

    view.rerender(
      <BubbleC
        voice={voice({ review })}
        interaction={{}}
        dragInteraction={{}}
        text={review.text}
        level={0}
        phase="completed"
      />,
    );
    expect(view.container.querySelector(".bubble-c-bubble.review")).toBeInTheDocument();
    expect(view.container.querySelectorAll(".bubble-c-wave.settled span")).toHaveLength(5);
    expect(screen.getByLabelText("语音识别结果确认")).toBeInTheDocument();
  });
});
