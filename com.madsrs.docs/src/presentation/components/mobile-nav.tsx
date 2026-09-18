"use client";

import { useState } from "react";

import type { DocumentationSummary } from "@/domain/documentation";

import { Sidebar } from "./sidebar";

export function MobileNav({ navigation, activeSlug }: Readonly<{ navigation: readonly DocumentationSummary[]; activeSlug: readonly string[] }>) {
  const [open, setOpen] = useState(false);

  return (
    <div className="mobile-nav">
      <button type="button" className="mobile-nav-trigger" aria-expanded={open} aria-controls="mobile-documentation-nav" onClick={() => setOpen((value) => !value)}>
        Menu
      </button>
      {open ? <Sidebar id="mobile-documentation-nav" navigation={navigation} activeSlug={activeSlug} /> : null}
    </div>
  );
}
