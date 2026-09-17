import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("next/navigation", () => ({ useRouter: () => ({ push: vi.fn() }) }));

import { DocsLayout } from "@/presentation/components/docs-layout";

describe("DocsLayout", () => {
  it("renders navigation, article content, and on-page headings as landmarks", () => {
    render(
      <DocsLayout
        navigation={[
          {
            slug: ["introduction", "quick-start"],
            title: "Quick start",
            description: "Create an app",
            section: "Getting started",
            order: 1,
          },
        ]}
        activeSlug={["introduction", "quick-start"]}
        headings={[{ id: "verify", text: "Verify", depth: 2 }]}
        searchEntries={[]}
      >
        <h1>Quick start</h1>
      </DocsLayout>,
    );

    expect(screen.getByRole("navigation", { name: /documentation/i })).toBeVisible();
    expect(screen.getByRole("main")).toHaveTextContent("Quick start");
    expect(screen.getByRole("navigation", { name: /on this page/i })).toHaveTextContent("Verify");
  });
});
