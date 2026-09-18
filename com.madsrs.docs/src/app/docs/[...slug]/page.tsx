import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { join } from "node:path";

import { ReadDocumentation } from "@/application/use-cases/read-documentation";
import { FileSystemDocumentationRepository } from "@/infrastructure/content/file-system-documentation-repository";
import { DocsLayout } from "@/presentation/components/docs-layout";
import { DocumentationPage } from "@/presentation/components/documentation-page";

interface DocumentationRouteProps {
  readonly params: Promise<{ slug: string[] }>;
}

function reader(): ReadDocumentation {
  return new ReadDocumentation(new FileSystemDocumentationRepository(join(process.cwd(), "content", "docs")));
}

export async function generateStaticParams(): Promise<{ slug: readonly string[] }[]> {
  return (await new FileSystemDocumentationRepository(join(process.cwd(), "content", "docs")).list()).map((page) => ({
    slug: page.slug,
  }));
}

export async function generateMetadata({ params }: DocumentationRouteProps): Promise<Metadata> {
  const { slug } = await params;
  const page = await reader().execute(slug);
  return page ? { title: page.page.title, description: page.page.description } : {};
}

export default async function DocumentationRoute({ params }: DocumentationRouteProps) {
  const { slug } = await params;
  const result = await reader().execute(slug);
  if (!result) {
    notFound();
  }
  const searchEntries = result.navigation.map((page) => ({
    title: page.title,
    slug: page.slug,
    description: page.description,
  }));

  return (
    <DocsLayout
      navigation={result.navigation}
      activeSlug={result.page.slug}
      headings={result.page.headings}
      searchEntries={searchEntries}
    >
      <DocumentationPage page={result.page} previous={result.previous} next={result.next} />
    </DocsLayout>
  );
}
