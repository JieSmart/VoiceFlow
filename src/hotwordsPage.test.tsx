import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { HotwordsPage } from "./pages/HotwordsPage";

describe("HotwordsPage", () => {
  it("labels the leftover cmd category as 其他", () => {
    render(<HotwordsPage />);
    expect(screen.getByRole("button", { name: "其他" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "分类" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "指令词" })).not.toBeInTheDocument();
  });
});
