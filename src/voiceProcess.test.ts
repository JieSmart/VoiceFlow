import { describe, expect, it } from "vitest";
import {
  bubbleAStage,
  bubbleAWindowSize,
  bubbleVisualState,
  bubbleVisualWindowSize,
  bubbleStyleWindowSize,
  bubbleWindowTransitionPlan,
  bubbleShouldActivateFromHidden,
  runBubbleWindowTransition,
  voiceProcessCopy,
} from "./voiceProcess";

describe("voice process status", () => {
  it("keeps user-facing states simple while internal phases advance", () => {
    expect(voiceProcessCopy("waking", false)[0]).toBe("正在唤醒");
    expect(voiceProcessCopy("recognizing", true)[0]).toBe("正在识别");
    expect(voiceProcessCopy("transforming", true)[0]).toBe("正在整理");
    expect(voiceProcessCopy("inserting", true)[0]).toBe("正在写入");
    expect(voiceProcessCopy("completed", true)[0]).toBe("已输入");
  });

  it("separates recording feedback from the processing transition", () => {
    expect(bubbleAStage("idle", false)).toBe("idle");
    expect(bubbleAStage("waking", false)).toBe("processing");
    expect(bubbleAStage("listening", false)).toBe("recording");
    expect(bubbleAStage("recognizing", false)).toBe("processing");
    expect(bubbleAStage("transforming", false)).toBe("processing");
    expect(bubbleAStage("inserting", false)).toBe("processing");
    expect(bubbleAStage("completed", false)).toBe("processing");
    expect(bubbleAStage("failed", false)).toBe("failed");
    expect(bubbleAStage("recognizing", true)).toBe("review");
    expect(bubbleShouldActivateFromHidden("idle", "recording")).toBe(true);
    expect(bubbleShouldActivateFromHidden("failed", "recording")).toBe(true);
    expect(bubbleShouldActivateFromHidden("failed", "failed")).toBe(false);
    expect(bubbleShouldActivateFromHidden("recording", "failed")).toBe(false);
  });

  it("uses the intended window size for every visual stage", () => {
    expect(bubbleAWindowSize("idle")).toEqual({ width: 390, height: 78 });
    expect(bubbleAWindowSize("processing")).toEqual({ width: 340, height: 92 });
    expect(bubbleAWindowSize("failed")).toEqual({ width: 420, height: 132 });
    expect(bubbleAWindowSize("recording")).toEqual({ width: 620, height: 116 });
    expect(bubbleAWindowSize("review")).toEqual({ width: 620, height: 360 });
    expect(bubbleAWindowSize("review", true)).toEqual({ width: 620, height: 420 });
  });

  it("uses style-specific window sizes for B and C animations", () => {
    expect(bubbleStyleWindowSize("B", bubbleVisualState("listening", false)))
      .toEqual({ width: 600, height: 370 });
    expect(bubbleStyleWindowSize("B", bubbleVisualState("completed", true, true)))
      .toEqual({ width: 640, height: 460 });
    expect(bubbleStyleWindowSize("C", bubbleVisualState("listening", false)))
      .toEqual({ width: 640, height: 210 });
    expect(bubbleStyleWindowSize("C", bubbleVisualState("completed", true, true)))
      .toEqual({ width: 660, height: 460 });
  });

  it("provides distinct copy for all processing outcomes", () => {
    expect(voiceProcessCopy("waking", false)).toEqual(["正在唤醒", "正在连接麦克风"]);
    expect(voiceProcessCopy("listening", false)).toEqual(["正在聆听", "等待你说话"]);
    expect(voiceProcessCopy("listening", true)).toEqual(["正在聆听", "正在实时识别"]);
    expect(voiceProcessCopy("recognizing", true)[0]).toBe("正在识别");
    expect(voiceProcessCopy("recognizing", true)[1]).toBe("正在生成最终文字，最长等待 60 秒");
    expect(voiceProcessCopy("recognizing", true, { streaming: true })[1]).toBe(
      "正在等待最终结果，最长约 22 秒",
    );
    expect(voiceProcessCopy("transforming", true)[0]).toBe("正在整理");
    expect(voiceProcessCopy("inserting", true)[0]).toBe("正在写入");
    expect(voiceProcessCopy("completed", true)).toEqual(["已输入", "文字已写入"]);
    expect(voiceProcessCopy("failed", true)).toEqual([
      "处理失败",
      "点击关闭，或按 Ctrl + Shift + Space 重试",
    ]);
    expect(voiceProcessCopy("idle", false, { wake: "Ctrl+Alt+Q" })).toEqual([
      "语流已待命",
      "按 Ctrl + Alt + Q 开始",
    ]);
  });

  it("builds expansion, shrink, mixed-axis, and same-size transition plans", () => {
    expect(
      bubbleWindowTransitionPlan(
        { width: 340, height: 92 },
        { width: 620, height: 360 },
      ),
    ).toMatchObject({
      envelopeSize: { width: 620, height: 360 },
      expandFirst: true,
      shrinkAfter: false,
    });
    expect(
      bubbleWindowTransitionPlan(
        { width: 620, height: 360 },
        { width: 340, height: 92 },
      ),
    ).toMatchObject({ expandFirst: false, shrinkAfter: true });
    expect(
      bubbleWindowTransitionPlan(
        { width: 390, height: 78 },
        { width: 340, height: 92 },
      ),
    ).toMatchObject({
      envelopeSize: { width: 390, height: 92 },
      expandFirst: true,
      shrinkAfter: true,
    });
    expect(
      bubbleWindowTransitionPlan(
        { width: 340, height: 92 },
        { width: 340, height: 92 },
      ),
    ).toMatchObject({ expandFirst: false, shrinkAfter: false });
  });

  it("derives review expansion from the displayed visual state", () => {
    const compact = bubbleVisualState("completed", true, false);
    const expanded = bubbleVisualState("transforming", true, true);
    expect(bubbleVisualWindowSize(compact)).toEqual({ width: 620, height: 360 });
    expect(bubbleVisualWindowSize(expanded)).toEqual({ width: 620, height: 420 });
  });

  it("keeps native expansion and shrink behind the content hand-off", async () => {
    const expansionEvents: string[] = [];
    await runBubbleWindowTransition(
      bubbleWindowTransitionPlan(
        { width: 340, height: 92 },
        { width: 620, height: 360 },
      ),
      {
        resize: async () => { expansionEvents.push("resize"); },
        leave: async () => { expansionEvents.push("leave"); },
        present: () => { expansionEvents.push("present"); },
        enter: async () => { expansionEvents.push("enter"); },
      },
    );
    expect(expansionEvents).toEqual(["leave", "resize", "present", "enter"]);

    const shrinkEvents: string[] = [];
    await runBubbleWindowTransition(
      bubbleWindowTransitionPlan(
        { width: 620, height: 360 },
        { width: 340, height: 92 },
      ),
      {
        resize: async () => { shrinkEvents.push("resize"); },
        leave: async () => { shrinkEvents.push("leave"); },
        present: () => { shrinkEvents.push("present"); },
        enter: async () => { shrinkEvents.push("enter"); },
      },
    );
    expect(shrinkEvents).toEqual(["leave", "resize", "present", "enter"]);
  });

  it("applies mixed-axis target bounds only while content is hidden", async () => {
    const events: string[] = [];
    await runBubbleWindowTransition(
      bubbleWindowTransitionPlan(
        { width: 390, height: 78 },
        { width: 340, height: 92 },
      ),
      {
        resize: async (size) => { events.push(`resize:${size.width}x${size.height}`); },
        leave: async () => { events.push("leave"); },
        present: () => { events.push("present"); },
        enter: async () => { events.push("enter"); },
      },
    );
    expect(events).toEqual([
      "leave",
      "resize:340x92",
      "present",
      "enter",
    ]);
  });
});
