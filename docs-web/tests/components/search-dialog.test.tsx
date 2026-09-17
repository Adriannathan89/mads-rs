import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

const mockPush = vi.fn();
vi.mock("next/navigation", () => ({ useRouter: () => ({ push: mockPush }) }));

import { SearchDialog } from "@/presentation/components/search-dialog";

describe("SearchDialog", () => {
  it("opens the search dialog and navigates to the selected result", async () => {
    const user = userEvent.setup();
    render(
      <SearchDialog
        entries={[{ title: "Quick start", slug: ["introduction", "quick-start"], description: "Create an app" }]}
      />,
    );

    await user.keyboard("{Control>}k{/Control}");
    await user.type(screen.getByRole("searchbox"), "quick");
    await user.click(screen.getByRole("option", { name: /quick start/i }));

    expect(mockPush).toHaveBeenCalledWith("/docs/introduction/quick-start");
  });
});
