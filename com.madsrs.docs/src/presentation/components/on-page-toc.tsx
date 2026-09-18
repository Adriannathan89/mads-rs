import type { Heading } from "@/domain/documentation";

export function OnPageToc({ headings }: Readonly<{ headings: readonly Heading[] }>) {
  if (headings.length === 0) {
    return null;
  }

  return (
    <nav className="on-page-toc" aria-label="On this page">
      <p>On this page</p>
      <ul>
        {headings.map((heading) => (
          <li key={heading.id} className={heading.depth === 3 ? "nested" : undefined}>
            <a href={`#${heading.id}`}>{heading.text}</a>
          </li>
        ))}
      </ul>
    </nav>
  );
}
