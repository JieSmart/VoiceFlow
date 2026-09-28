export type RecordingCueKind = "start" | "stop";

export interface RecordingCueGateState {
  lastStartSessionId: string | null;
  lastStopSessionId: string | null;
}

interface OscillatorLike {
  type: string;
  frequency: { value: number };
  connect: (node: unknown) => void;
  start: (when: number) => void;
  stop: (when: number) => void;
}

interface GainLike {
  gain: {
    setValueAtTime: (value: number, when: number) => void;
    exponentialRampToValueAtTime: (value: number, when: number) => void;
  };
  connect: (node: unknown) => void;
}

export interface CueAudioContext {
  currentTime: number;
  state?: string;
  destination: unknown;
  resume?: () => Promise<void>;
  createOscillator: () => OscillatorLike;
  createGain: () => GainLike;
}

const CUES: Record<RecordingCueKind, { frequency: number; durationMs: number }> = {
  start: { frequency: 880, durationMs: 70 },
  stop: { frequency: 523.25, durationMs: 90 },
};

let sharedContext: CueAudioContext | null = null;
let gate = initialRecordingCueGate();

export function initialRecordingCueGate(): RecordingCueGateState {
  return { lastStartSessionId: null, lastStopSessionId: null };
}

export function shouldPlayRecordingCue(enabled: boolean) {
  return enabled === true;
}

export function shouldPlayRecordingCueInWindow(
  isTauriRuntime: boolean,
  isBubbleWindow: boolean,
) {
  return !isTauriRuntime || isBubbleWindow;
}

export function takeRecordingCue(
  state: RecordingCueGateState,
  kind: RecordingCueKind,
  sessionId: string,
  enabled: boolean,
) {
  if (!shouldPlayRecordingCue(enabled) || !sessionId) return false;
  if (kind === "start") {
    if (state.lastStartSessionId === sessionId) return false;
    state.lastStartSessionId = sessionId;
    state.lastStopSessionId = null;
    return true;
  }
  if (state.lastStopSessionId === sessionId) return false;
  state.lastStopSessionId = sessionId;
  return true;
}

export function createCueAudioContext(): CueAudioContext | null {
  const Ctor =
    typeof window === "undefined"
      ? undefined
      : window.AudioContext ||
        (window as Window & { webkitAudioContext?: typeof AudioContext })
          .webkitAudioContext;
  if (!Ctor) return null;
  return new Ctor() as unknown as CueAudioContext;
}

export function getSharedCueAudioContext(
  factory: () => CueAudioContext | null = createCueAudioContext,
) {
  sharedContext ??= factory();
  return sharedContext;
}

export function resetRecordingCueGate() {
  gate = initialRecordingCueGate();
}

export function resetCueAudioContext() {
  sharedContext = null;
}

function startCueTone(kind: RecordingCueKind, ctx: CueAudioContext) {
  const cue = CUES[kind];
  const oscillator = ctx.createOscillator();
  const gain = ctx.createGain();
  oscillator.type = "sine";
  oscillator.frequency.value = cue.frequency;
  const now = ctx.currentTime;
  const end = now + cue.durationMs / 1000;
  gain.gain.setValueAtTime(0.0001, now);
  gain.gain.exponentialRampToValueAtTime(0.045, now + 0.008);
  gain.gain.exponentialRampToValueAtTime(0.0001, end);
  oscillator.connect(gain);
  gain.connect(ctx.destination);
  oscillator.start(now);
  oscillator.stop(end + 0.02);
}

export async function playRecordingCueTone(
  kind: RecordingCueKind,
  contextFactory: () => CueAudioContext | null = createCueAudioContext,
) {
  try {
    const ctx = getSharedCueAudioContext(contextFactory);
    if (!ctx) return;
    if (ctx.state === "suspended" && ctx.resume) {
      await ctx.resume();
    }
    startCueTone(kind, ctx);
  } catch {
    // Cue playback must never interrupt recording.
  }
}

export async function playRecordingCue(
  kind: RecordingCueKind,
  options: {
    enabled: boolean;
    sessionId: string;
    inWindow: boolean;
  },
  contextFactory?: () => CueAudioContext | null,
) {
  if (!options.inWindow) return;
  if (!takeRecordingCue(gate, kind, options.sessionId, options.enabled)) return;
  await playRecordingCueTone(kind, contextFactory);
}
