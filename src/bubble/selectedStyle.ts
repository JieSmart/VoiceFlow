import { isTauri } from "../api";
import type { AppSettings } from "../types";

export function selectedBubbleStyle(settings: AppSettings) {
  const preview = !isTauri()
    ? new URLSearchParams(location.search).get("previewStyle")
    : null;
  return preview === "A" || preview === "B" || preview === "C"
    ? preview
    : settings.bubbleStyle;
}
