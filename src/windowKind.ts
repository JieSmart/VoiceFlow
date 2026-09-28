export function isBubbleWindow() {
  return /(?:^|\/)bubble\.html(?:[?#]|$)/.test(location.pathname)
    || new URLSearchParams(location.search).get("window") === "bubble";
}
