import { readdir, readFile } from "node:fs/promises";
import { relative, resolve, sep } from "node:path";

import matter from "gray-matter";

import type { DocumentationRepository } from "@/application/ports/documentation-repository";
import type {
  DocumentationPage,
  DocumentationSlug,
  DocumentationSummary,
  Heading,
  SearchEntry,
} from "@/domain/documentation";

interface ContentFile {
  readonly path: string;
  readonly slug: readonly string[];
}

function slugKey(slug: DocumentationSlug): string {
  return slug.join("/");
}

function createHeadingId(text: string, usedIds: Map<string, number>): string {
  const base = text
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9\s-]/g, "")
    .replace(/\s+/g, "-")
    .replace(/-+/g, "-") || "section";
  const seen = usedIds.get(base) ?? 0;
  usedIds.set(base, seen + 1);
  return seen === 0 ? base : `${base}-${seen}`;
}

function extractHeadings(source: string): Heading[] {
  const usedIds = new Map<string, number>();
  const headings: Heading[] = [];

  for (const match of source.matchAll(/^(#{2,3})\s+(.+?)\s*#*\s*$/gm)) {
    const depth = match[1].length as 2 | 3;
    const text = match[2].replace(/`/g, "").trim();
    headings.push({ id: createHeadingId(text, usedIds), text, depth });
  }

  return headings;
}

function asFrontmatter(data: Record<string, unknown>, path: string): Omit<DocumentationSummary, "slug"> {
  const { title, description, section, order } = data;
  if (typeof title !== "string" || !title.trim()) {
    throw new Error(`Invalid frontmatter in ${path}: title is required`);
  }
  if (typeof description !== "string" || !description.trim()) {
    throw new Error(`Invalid frontmatter in ${path}: description is required`);
  }
  if (typeof section !== "string" || !section.trim()) {
    throw new Error(`Invalid frontmatter in ${path}: section is required`);
  }
  if (typeof order !== "number" || !Number.isInteger(order) || order < 1) {
    throw new Error(`Invalid frontmatter in ${path}: order must be a positive integer`);
  }

  return { title: title.trim(), description: description.trim(), section: section.trim(), order };
}

function toPlainText(source: string): string {
  return source
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/[`*_>#()!]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function sortByNavigation(left: DocumentationSummary, right: DocumentationSummary): number {
  return (
    left.section.localeCompare(right.section) ||
    left.order - right.order ||
    left.title.localeCompare(right.title)
  );
}

export class FileSystemDocumentationRepository implements DocumentationRepository {
  private readonly directory: string;

  public constructor(contentDirectory: string) {
    this.directory = resolve(contentDirectory);
  }

  public async list(): Promise<DocumentationSummary[]> {
    const pages = await this.catalog();
    return pages.map(({ slug, title, description, section, order }) => ({
      slug,
      title,
      description,
      section,
      order,
    }));
  }

  public async get(slug: DocumentationSlug): Promise<DocumentationPage | null> {
    if (!this.isSafeSlug(slug)) {
      return null;
    }

    const page = (await this.catalog()).find((candidate) => slugKey(candidate.slug) === slugKey(slug));
    return page ?? null;
  }

  public async search(query: string): Promise<SearchEntry[]> {
    const normalizedQuery = query.trim().toLowerCase();
    if (!normalizedQuery) {
      return [];
    }

    return (await this.catalog())
      .filter((page) => {
        const haystack = [page.title, page.description, toPlainText(page.source), ...page.headings.map((h) => h.text)]
          .join(" ")
          .toLowerCase();
        return haystack.includes(normalizedQuery);
      })
      .map((page) => ({
        title: page.title,
        slug: page.slug,
        description: page.description,
        matchedHeading: page.headings.find((heading) => heading.text.toLowerCase().includes(normalizedQuery))?.text,
      }));
  }

  private async catalog(): Promise<DocumentationPage[]> {
    const files = await this.findContentFiles(this.directory);
    const pages = await Promise.all(files.map((file) => this.readPage(file)));
    return pages.sort(sortByNavigation);
  }

  private async findContentFiles(directory: string): Promise<ContentFile[]> {
    const entries = await readdir(directory, { withFileTypes: true });
    const nested = await Promise.all(
      entries.map(async (entry): Promise<ContentFile[]> => {
        const path = resolve(directory, entry.name);
        if (entry.isDirectory()) {
          return this.findContentFiles(path);
        }
        if (!entry.isFile() || !entry.name.endsWith(".mdx")) {
          return [];
        }
        const relativePath = relative(this.directory, path);
        return [{
          path,
          slug: relativePath.slice(0, -4).split(sep),
        }];
      }),
    );
    return nested.flat();
  }

  private async readPage(file: ContentFile): Promise<DocumentationPage> {
    const parsed = matter(await readFile(file.path, "utf8"));
    const frontmatter = asFrontmatter(parsed.data, file.path);
    return {
      ...frontmatter,
      slug: file.slug,
      headings: extractHeadings(parsed.content),
      source: parsed.content.trim(),
    };
  }

  private isSafeSlug(slug: DocumentationSlug): boolean {
    return slug.length > 0 && slug.every((segment) => /^[a-z0-9][a-z0-9-]*$/i.test(segment));
  }
}
