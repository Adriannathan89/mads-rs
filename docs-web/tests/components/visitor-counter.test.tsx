import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { VisitorCounter } from "@/presentation/components/visitor-counter";

describe("VisitorCounter", () => {
  it("shows an unavailable visitor count without hiding the documentation CTA", () => {
    render(<VisitorCounter state={{ kind: "unavailable" }} />);

    expect(screen.getByText("Visitor count unavailable")).toBeVisible();
    expect(screen.getByRole("link", { name: /get started/i })).toBeVisible();
  });
});
