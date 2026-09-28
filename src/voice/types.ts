import type { VoiceController } from "./voiceController";

export type VoiceSession = ReturnType<typeof VoiceController>;
export type BubbleInteraction = Record<string, unknown>;
