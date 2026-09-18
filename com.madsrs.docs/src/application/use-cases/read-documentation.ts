import type { DocumentationRepository } from "@/application/ports/documentation-repository";
import type { DocumentationPage, DocumentationSlug, DocumentationSummary } from "@/domain/documentation";

export interface DocumentationReader {
  readonly page: DocumentationPage;
  readonly navigation: readonly DocumentationSummary[];
  readonly previous: DocumentationSummary | null;
  readonly next: DocumentationSummary | null;
}

export class ReadDocumentation {
  public constructor(private readonly repository: DocumentationRepository) {}

  public async execute(slug: DocumentationSlug): Promise<DocumentationReader | null> {
    const [page, navigation] = await Promise.all([this.repository.get(slug), this.repository.list()]);

    if (!page) {
      return null;
    }

    const index = navigation.findIndex((entry) => entry.slug.join("/") === page.slug.join("/"));
    return {
      page,
      navigation,
      previous: index > 0 ? navigation[index - 1] : null,
      next: index >= 0 && index < navigation.length - 1 ? navigation[index + 1] : null,
    };
  }
}
