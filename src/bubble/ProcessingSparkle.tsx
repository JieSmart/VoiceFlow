import { lazy, Suspense } from "react";

const ProcessingSparkleLottie = lazy(async () => {
  const [{ LottieSvg }, animation] = await Promise.all([
    import("lottie-react"),
    import("../assets/ai-generate.json"),
  ]);
  const data = "default" in animation ? animation.default : animation;
  return {
    default: function ProcessingSparkleLottie() {
      return (
        <LottieSvg
          src={data}
          loop
          autoplay
          className="processing-sparkle-player"
        />
      );
    },
  };
});

export function ProcessingSparkle({ size = 18 }: { size?: number }) {
  return (
    <span
      className="processing-sparkle"
      style={{ width: size, height: size }}
      aria-hidden="true"
    >
      <span className="processing-sparkle-lottie">
        <Suspense fallback={null}>
          <ProcessingSparkleLottie />
        </Suspense>
      </span>
    </span>
  );
}
