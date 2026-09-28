import { describe, expect, it } from "vitest";
import {
  initialVoiceActivityState,
  markVoiceActivitySpeech,
  sampleVoiceActivity,
  shouldAutoFinishVoiceActivity,
  SILENCE_TIMEOUT_MS,
} from "./voiceActivity";

describe("voice activity detection", () => {
  it("does not stop before the user has spoken", () => {
    const state = initialVoiceActivityState();
    for (let now = 0; now <= 2_000; now += 50) {
      expect(sampleVoiceActivity(state, 0.001, now)).toBe(false);
    }
    expect(state.speechStarted).toBe(false);
  });

  it("stops after 1.5 seconds of silence following confirmed speech", () => {
    const state = initialVoiceActivityState();
    for (let now = 0; now <= 200; now += 50) {
      sampleVoiceActivity(state, 0.03, now);
    }
    expect(state.speechStarted).toBe(true);
    expect(sampleVoiceActivity(state, 0.001, 200 + SILENCE_TIMEOUT_MS - 1)).toBe(false);
    expect(sampleVoiceActivity(state, 0.001, 200 + SILENCE_TIMEOUT_MS)).toBe(true);
  });

  it("resets the silence timer when speech resumes", () => {
    const state = initialVoiceActivityState();
    for (let now = 0; now <= 200; now += 50) {
      sampleVoiceActivity(state, 0.03, now);
    }
    expect(sampleVoiceActivity(state, 0.001, 1_400)).toBe(false);
    expect(sampleVoiceActivity(state, 0.02, 1_450)).toBe(false);
    expect(sampleVoiceActivity(state, 0.02, 1_520)).toBe(false);
    expect(sampleVoiceActivity(state, 0.001, 3_019)).toBe(false);
    expect(sampleVoiceActivity(state, 0.001, 3_020)).toBe(true);
  });

  it("calibrates to steady room noise instead of treating it as speech", () => {
    const state = initialVoiceActivityState();
    for (let now = 0; now <= 2_000; now += 35) {
      expect(sampleVoiceActivity(state, 0.012, now)).toBe(false);
    }
    expect(state.speechStarted).toBe(false);
    expect(state.noiseFloor).toBeGreaterThan(0.01);
  });

  it("ignores a single noise spike during trailing silence", () => {
    const state = initialVoiceActivityState();
    markVoiceActivitySpeech(state, 200);
    expect(sampleVoiceActivity(state, 0.001, 1_000)).toBe(false);
    expect(sampleVoiceActivity(state, 0.08, 1_100)).toBe(false);
    expect(sampleVoiceActivity(state, 0.001, 1_699)).toBe(false);
    expect(sampleVoiceActivity(state, 0.001, 1_700)).toBe(true);
  });

  it("accepts ASR text as proof that speech has started", () => {
    const state = initialVoiceActivityState();
    markVoiceActivitySpeech(state, 500);
    expect(state.speechStarted).toBe(true);
    expect(sampleVoiceActivity(state, 0.001, 1_999)).toBe(false);
    expect(sampleVoiceActivity(state, 0.001, 2_000)).toBe(true);
  });

  it("never auto-finishes while silence ending is disabled", () => {
    const state = initialVoiceActivityState();
    for (let now = 0; now <= 200; now += 50) {
      expect(shouldAutoFinishVoiceActivity(false, state, 0.03, now)).toBe(false);
    }
    expect(
      shouldAutoFinishVoiceActivity(false, state, 0.001, 200 + SILENCE_TIMEOUT_MS * 2),
    ).toBe(false);
    expect(state.speechStarted).toBe(false);
  });
});
