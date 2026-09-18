export type DocumentationSlug = readonly string[];

export interface Heading {
  readonly id: string;
  readonly text: string;
  readonly depth: 2 | 3;
}

export interface DocumentationSummary {
  readonly slug: DocumentationSlug;
  readonly title: string;
  readonly description: string;
  readonly section: string;
  readonly order: number;
}

export interface DocumentationPage extends DocumentationSummary {
  readonly headings: readonly Heading[];
  readonly source: string;
}

export interface SearchEntry {
  readonly title: string;
  readonly slug: DocumentationSlug;
  readonly description: string;
  readonly matchedHeading?: string;
}
