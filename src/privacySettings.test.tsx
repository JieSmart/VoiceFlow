import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PrivacySettings } from "./settings/PrivacySettings";
import * as api from "./api";

vi.mock("./api", async (loadOriginal) => {
  const original = await loadOriginal<typeof import("./api")>();
  return {
    ...original,
    getDataDirectory: vi.fn(),
    chooseDataDirectory: vi.fn(),
    migrateDataDirectory: vi.fn(),
    quitForDataDirectoryRestart: vi.fn(),
    clearRecords: vi.fn(),
  };
});

describe("PrivacySettings", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    vi.mocked(api.getDataDirectory).mockResolvedValue({
      currentPath: "C:\\old",
      defaultPath: "C:\\old",
      isCustom: false,
    });
    vi.mocked(api.chooseDataDirectory).mockResolvedValue({
      parentPath: "D:\\Data",
      targetPath: "D:\\Data\\语流",
    });
    vi.mocked(api.migrateDataDirectory).mockResolvedValue({
      oldPath: "C:\\old",
      newPath: "D:\\Data\\语流",
      restartRequired: true,
    });
  });

  it("confirms the target and reports that restart is required", async () => {
    render(<PrivacySettings />);
    expect(await screen.findByText("C:\\old")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "选择新位置" }));
    expect(await screen.findByText("D:\\Data\\语流")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "开始迁移" }));

    expect(await screen.findByText("迁移完成，请重启语流")).toBeInTheDocument();
    expect(api.migrateDataDirectory).toHaveBeenCalledWith("D:\\Data");
  });

  it("does not style a successful record wipe as an error", async () => {
    vi.mocked(api.clearRecords).mockResolvedValue(undefined);
    render(<PrivacySettings />);
    expect(await screen.findByText("C:\\old")).toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "立即清空" })[0]);
    const notice = await screen.findByText("已清空输入记录");
    expect(notice).toHaveClass("success");
    expect(notice).not.toHaveClass("error");
  });
});
