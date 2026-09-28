export interface VoiceActivityState {
  noiseFloor: number;
  calibrationStartedAt: number | null;
  voiceCandidateSince: number | null;
  resumeCandidateSince: number | null;
  speechStarted: boolean;
  lastVoiceAt: number | null;
}

export const SILENCE_TIMEOUT_MS = 1_500;
const CALIBRATION_MS = 350;
const SPEECH_CONFIRM_MS = 140;
const RESUME_CONFIRM_MS = 70;
const IMMEDIATE_SPEECH_RMS = 0.025;

export function initialVoiceActivityState(): VoiceActivityState {
  return {
    noiseFloor: 0.002,
    calibrationStartedAt: null,
    voiceCandidateSince: null,
    resumeCandidateSince: null,
    speechStarted: false,
    lastVoiceAt: null,
  };
}

export function markVoiceActivitySpeech(
  state: VoiceActivityState,
  now: number,
) {
  state.speechStarted = true;
  state.voiceCandidateSince = null;
  // Speech has already been confirmed by either sustained energy or ASR text,
  // so the next active sample may refresh the hangover immediately.
  state.resumeCandidateSince = now - RESUME_CONFIRM_MS;
  state.lastVoiceAt = now;
}

export function sampleVoiceActivity(
  state: VoiceActivityState,
  rms: number,
  now: number,
): boolean {
  const sample = Number.isFinite(rms) ? Math.max(0, rms) : 0;
  state.calibrationStartedAt ??= now;

  if (!state.speechStarted) {
    const calibrating = now - state.calibrationStartedAt < CALIBRATION_MS;
    if (calibrating && sample < IMMEDIATE_SPEECH_RMS) {
      const alpha = sample < state.noiseFloor ? 0.22 : 0.12;
      state.noiseFloor += (sample - state.noiseFloor) * alpha;
      state.voiceCandidateSince = null;
      return false;
    }

    const startThreshold = Math.max(0.006, state.noiseFloor * 1.7);
    if (sample >= startThreshold) {
      state.voiceCandidateSince ??= now;
      if (now - state.voiceCandidateSince >= SPEECH_CONFIRM_MS) {
        markVoiceActivitySpeech(state, now);
      }
    } else {
      state.voiceCandidateSince = null;
      const alpha = sample < state.noiseFloor ? 0.12 : 0.025;
      state.noiseFloor += (sample - state.noiseFloor) * alpha;
    }
    return false;
  }

  const continueThreshold = Math.max(0.0045, state.noiseFloor * 1.35);
  if (sample >= continueThreshold) {
    state.resumeCandidateSince ??= now;
    if (now - state.resumeCandidateSince >= RESUME_CONFIRM_MS) {
      state.lastVoiceAt = now;
    }
  } else {
    state.resumeCandidateSince = null;
    const alpha = sample < state.noiseFloor ? 0.08 : 0.015;
    state.noiseFloor += (sample - state.noiseFloor) * alpha;
  }

  return state.lastVoiceAt !== null && now - state.lastVoiceAt >= SILENCE_TIMEOUT_MS;
}

export function shouldAutoFinishVoiceActivity(
  enabled: boolean,
  state: VoiceActivityState,
  rms: number,
  now: number,
): boolean {
  return enabled && sampleVoiceActivity(state, rms, now);
}
