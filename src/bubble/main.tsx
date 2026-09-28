import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../styles.css";
import { useSharedBackendState } from "../hooks/useSharedBackendState";
import { BubbleApp } from "./BubbleApp";

function BubbleRoot() {
  useSharedBackendState();
  return <BubbleApp />;
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BubbleRoot />
  </StrictMode>,
);
