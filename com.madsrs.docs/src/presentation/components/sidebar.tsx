import Link from "next/link";

import type { DocumentationSummary } from "@/domain/documentation";

interface SidebarProps {
  readonly navigation: readonly DocumentationSummary[];
  readonly activeSlug: readonly string[];
  readonly id?: string;
}

export function Sidebar({ navigation, activeSlug, id }: SidebarProps) {
  const active = activeSlug.join("/");
  const sections = navigation.reduce<Map<string, DocumentationSummary[]>>((grouped, page) => {
    const pages = grouped.get(page.section) ?? [];
    pages.push(page);
    grouped.set(page.section, pages);
    return grouped;
  }, new Map());

  return (
    <nav id={id} className="docs-sidebar" aria-label="Documentation">
      {[...sections.entries()].map(([section, pages]) => (
        <section key={section} className="sidebar-section" aria-labelledby={`section-${section}`}>
          <h2 id={`section-${section}`}>{section}</h2>
          <ul>
            {pages.map((page) => {
              const href = `/docs/${page.slug.join("/")}`;
              const current = active === page.slug.join("/");
              return (
                <li key={href}>
                  <Link href={href} aria-current={current ? "page" : undefined} className={current ? "active" : undefined}>
                    {page.title}
                  </Link>
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </nav>
  );
}
