import type { ReactNode } from "react";

import type { DocumentationSummary, Heading, SearchEntry } from "@/domain/documentation";

import { Header } from "./header";
import { MobileNav } from "./mobile-nav";
import { OnPageToc } from "./on-page-toc";
import { Sidebar } from "./sidebar";

interface DocsLayoutProps {
  readonly navigation: readonly DocumentationSummary[];
  readonly activeSlug: readonly string[];
  readonly headings: readonly Heading[];
  readonly searchEntries: readonly SearchEntry[];
  readonly children: ReactNode;
}

export function DocsLayout({ navigation, activeSlug, headings, searchEntries, children }: DocsLayoutProps) {
  return (
    <>
      <Header searchEntries={searchEntries} />
      <div className="docs-mobile-header">
        <MobileNav navigation={navigation} activeSlug={activeSlug} />
      </div>
      <div className="docs-layout">
        <aside className="docs-sidebar-column"><Sidebar navigation={navigation} activeSlug={activeSlug} /></aside>
        <main className="docs-main">{children}</main>
        <aside className="docs-toc-column"><OnPageToc headings={headings} /></aside>
      </div>
    </>
  );
}
