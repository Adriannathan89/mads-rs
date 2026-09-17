import type {
  DocumentationPage,
  DocumentationSlug,
  DocumentationSummary,
  SearchEntry,
} from "@/domain/documentation";

export interface DocumentationRepository {
  list(): Promise<DocumentationSummary[]>;
  get(slug: DocumentationSlug): Promise<DocumentationPage | null>;
  search(query: string): Promise<SearchEntry[]>;
}
