import { MDXRemote } from "next-mdx-remote/rsc";
import remarkGfm from "remark-gfm";
import rehypeSlug from "rehype-slug";

import type { DocumentationPage as DocumentationPageRecord, DocumentationSummary } from "@/domain/documentation";

import { mdxComponents } from "./mdx-components";

interface DocumentationPageProps {
  readonly page: DocumentationPageRecord;
  readonly previous: DocumentationSummary | null;
  readonly next: DocumentationSummary | null;
}

function pageLink(page: DocumentationSummary): string {
  return `/docs/${page.slug.join("/")}`;
}

export function DocumentationPage({ page, previous, next }: DocumentationPageProps) {
  return (
    <article className="documentation-page">
      <p className="eyebrow">{page.section}</p>
      <h1>{page.title}</h1>
      <p className="lead">{page.description}</p>
      <MDXRemote
        source={page.source}
        options={{
          mdxOptions: {
            remarkPlugins: [remarkGfm],
            rehypePlugins: [rehypeSlug],
          },
        }}
        components={mdxComponents}
      />
      <nav className="page-pagination" aria-label="Documentation pagination">
        {previous ? <a href={pageLink(previous)}>← {previous.title}</a> : <span />}
        {next ? <a href={pageLink(next)}>{next.title} →</a> : <span />}
      </nav>
    </article>
  );
}
