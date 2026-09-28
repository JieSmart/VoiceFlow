export interface NamedAudioDevice {
  id: string;
  name: string;
}

export function resolvedMicrophoneId(
  saved: string | undefined,
  devices: NamedAudioDevice[],
) {
  if (!saved) return undefined;
  if (devices.some((device) => device.id === saved)) return saved;
  const index = Number.parseInt(saved, 10);
  if (Number.isInteger(index) && index >= 0) {
    return devices[index]?.id;
  }
  return saved;
}
