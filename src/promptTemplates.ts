export interface PromptTemplate {
  id: number;
  name: string;
  triggerWord: string;
  prompt: string;
  enabled: boolean;
  isDefault: boolean;
}

export const FALLBACK_PROMPT =
  "整理这段语音转写，使其清晰、简洁并保留原意。不要添加原文没有的信息，只输出整理后的正文。";

export function interpolatePrompt(
  template: string,
  vars: { text: string; hotwords: string[]; datetime: string },
) {
  return template
    .replaceAll("{{text}}", vars.text)
    .replaceAll("{{hotwords}}", vars.hotwords.join("、"))
    .replaceAll("{{datetime}}", vars.datetime);
}

export function resolvePromptTemplate(
  templates: PromptTemplate[],
  text: string,
  defaultId?: number,
) {
  const enabled = templates.filter((item) => item.enabled && item.prompt.trim());
  const hits = enabled
    .filter((item) => item.triggerWord.trim() && text.includes(item.triggerWord.trim()))
    .sort((a, b) => b.triggerWord.trim().length - a.triggerWord.trim().length);
  return (
    hits[0] ||
    enabled.find((item) => item.id === defaultId) ||
    enabled.find((item) => item.isDefault) ||
    enabled[0] ||
    null
  );
}

export function promptRequest(
  template: PromptTemplate | null,
  text: string,
  hotwords: string[],
  datetime = new Date().toLocaleString("zh-CN", { hour12: false }),
) {
  const source = template?.prompt.trim() || FALLBACK_PROMPT;
  const rendered = interpolatePrompt(source, { text, hotwords, datetime });
  if (source.includes("{{text}}")) {
    return { instructions: rendered, input: text };
  }
  return { instructions: rendered, input: text };
}
