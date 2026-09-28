import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defaultSettings, useAppStore } from "../store";
import { SettingsModal } from "./SettingsModal";

vi.mock("../api", () => ({
  saveSettings: vi.fn(() => Promise.resolve()),
  saveHotword: vi.fn(() => Promise.resolve()),
  removeHotword: vi.fn(() => Promise.resolve()),
}));

describe("SettingsModal", () => {
  beforeEach(() => {
    useAppStore.setState({
      settingsOpen: true,
      settingsTab: "general",
      settings: defaultSettings,
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("closes when clicking the dimmed area outside the panel", () => {
    const { container } = render(<SettingsModal />);
    expect(screen.getByText("语流设置")).toBeInTheDocument();
    fireEvent.mouseDown(container.querySelector(".backdrop")!);
    expect(useAppStore.getState().settingsOpen).toBe(false);
  });

  it("stays open when clicking inside the panel", () => {
    const { container } = render(<SettingsModal />);
    fireEvent.mouseDown(container.querySelector(".settings-modal")!);
    expect(useAppStore.getState().settingsOpen).toBe(true);
    expect(screen.getByText("语流设置")).toBeInTheDocument();
  });
});
