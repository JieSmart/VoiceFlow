import { describe, expect, it, beforeEach } from "vitest";
import {
  initialRecordingCueGate,
  playRecordingCue,
  playRecordingCueTone,
  resetCueAudioContext,
  resetRecordingCueGate,
  shouldPlayRecordingCue,
  shouldPlayRecordingCueInWindow,
  takeRecordingCue,
  type CueAudioContext,
} from "./cueSounds";

function mockAudioContext(state = "running") {
  let resumeCalls = 0;
  const oscillator = {
    type: "",
    frequency: { value: 0 },
    connect: () => undefined,
    start: () => undefined,
    stop: () => undefined,
  };
  const gain = {
    gain: {
      setValueAtTime: () => undefined,
      exponentialRampToValueAtTime: () => undefined,
    },
    connect: () => undefined,
  };
  const ctx: CueAudioContext = {
    currentTime: 0,
    state,
    destination: {},
    resume: async () => {
      resumeCalls += 1;
      ctx.state = "running";
    },
    createOscillator: () => oscillator,
    createGain: () => gain,
  };
  return { ctx, oscillator, resumeCount: () => resumeCalls };
}

describe("recording cue sounds", () => {
  beforeEach(() => {
    resetRecordingCueGate();
    resetCueAudioContext();
  });

  it("plays only when the setting is on", () => {
    expect(shouldPlayRecordingCue(true)).toBe(true);
    expect(shouldPlayRecordingCue(false)).toBe(false);
  });

  it("plays in the browser preview or the bubble window, not the Tauri main window", () => {
    expect(shouldPlayRecordingCueInWindow(true, true)).toBe(true);
    expect(shouldPlayRecordingCueInWindow(true, false)).toBe(false);
    expect(shouldPlayRecordingCueInWindow(false, false)).toBe(true);
  });

  it("dedupes start and stop for the same session", () => {
    const state = initialRecordingCueGate();
    expect(takeRecordingCue(state, "start", "s1", true)).toBe(true);
    expect(takeRecordingCue(state, "start", "s1", true)).toBe(false);
    expect(takeRecordingCue(state, "stop", "s1", true)).toBe(true);
    expect(takeRecordingCue(state, "stop", "s1", true)).toBe(false);
    expect(takeRecordingCue(state, "start", "s2", true)).toBe(true);
  });

  it("keeps only the first stop cue for a session", () => {
    const state = initialRecordingCueGate();
    expect(takeRecordingCue(state, "stop", "s1", true)).toBe(true);
    expect(takeRecordingCue(state, "stop", "s1", true)).toBe(false);
  });

  it("does not play when disabled or session id is empty", () => {
    const state = initialRecordingCueGate();
    expect(takeRecordingCue(state, "start", "s1", false)).toBe(false);
    expect(takeRecordingCue(state, "start", "", true)).toBe(false);
  });

  it("uses a higher start pitch than stop", async () => {
    const start = mockAudioContext();
    resetCueAudioContext();
    await playRecordingCueTone("start", () => start.ctx);
    resetCueAudioContext();
    const stop = mockAudioContext();
    await playRecordingCueTone("stop", () => stop.ctx);
    expect(start.oscillator.frequency.value).toBeGreaterThan(
      stop.oscillator.frequency.value,
    );
  });

  it("skips playback outside the cue window", async () => {
    const audio = mockAudioContext();
    await playRecordingCue(
      "start",
      { enabled: true, sessionId: "s1", inWindow: false },
      () => audio.ctx,
    );
    expect(audio.oscillator.frequency.value).toBe(0);
  });

  it("reuses one audio context and resumes it when suspended", async () => {
    let created = 0;
    const audio = mockAudioContext("suspended");
    const factory = () => {
      created += 1;
      return audio.ctx;
    };
    await playRecordingCue(
      "start",
      { enabled: true, sessionId: "s1", inWindow: true },
      factory,
    );
    await playRecordingCue(
      "stop",
      { enabled: true, sessionId: "s1", inWindow: true },
      factory,
    );
    expect(created).toBe(1);
    expect(audio.resumeCount()).toBe(1);
    expect(audio.ctx.state).toBe("running");
  });
});
