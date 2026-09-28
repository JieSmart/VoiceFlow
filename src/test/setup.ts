import { createElement } from "react";
import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

vi.mock("lottie-react", () => ({
  Lottie: (props: { className?: string }) =>
    createElement("span", { className: props.className }),
  LottieSvg: (props: { className?: string }) =>
    createElement("span", { className: props.className }),
}));

Object.defineProperty(globalThis, "ResizeObserver", {
  value: class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  },
});

Object.defineProperty(HTMLCanvasElement.prototype, "getContext", {
  configurable: true,
  value() {
    const context = {
      canvas: this as HTMLCanvasElement,
      fillStyle: "",
      strokeStyle: "",
      lineWidth: 1,
      fillRect() {},
      clearRect() {},
      getImageData() {
        return { data: [] };
      },
      putImageData() {},
      createImageData() {
        return [];
      },
      setTransform() {},
      resetTransform() {},
      drawImage() {},
      save() {},
      fillText() {},
      restore() {},
      beginPath() {},
      moveTo() {},
      lineTo() {},
      closePath() {},
      stroke() {},
      translate() {},
      scale() {},
      rotate() {},
      arc() {},
      fill() {},
      measureText() {
        return { width: 0 };
      },
      transform() {},
      rect() {},
      clip() {},
    };
    return context;
  },
});

Object.defineProperties(HTMLElement.prototype, {
  setPointerCapture: { value() {} },
  releasePointerCapture: { value() {} },
  hasPointerCapture: { value() { return true; } },
});
