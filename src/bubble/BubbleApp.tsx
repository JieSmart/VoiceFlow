import { VoiceController } from "../voice/voiceController";
import { ContextMenuHost } from "../ui/ContextMenuHost";
import { VoiceBubble } from "./VoiceBubble";

export function BubbleApp() {
  const voice = VoiceController();
  return (
    <ContextMenuHost className="bubble-window" swallowOnly>
      <VoiceBubble voice={voice} />
    </ContextMenuHost>
  );
}
