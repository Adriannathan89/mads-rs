import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { FileSystemDocumentationRepository } from "@/infrastructure/content/file-system-documentation-repository";

describe("MADS documentation catalog", () => {
  it("publishes every required section with ordered page metadata", async () => {
    const repository = new FileSystemDocumentationRepository(join(process.cwd(), "content", "docs"));
    const pages = await repository.list();

    expect(pages.map((page) => page.slug.join("/"))).toEqual(expect.arrayContaining([
      "introduction/quick-start",
      "build-an-api/validation",
      "configuration/typed-configuration-and-secrets",
      "data-and-security/postgres-and-diesel",
      "guides/build-a-user-api",
    ]));
    expect(new Set(pages.map((page) => page.slug.join("/")))).toHaveLength(pages.length);
    expect(await repository.get(["guides", "build-a-user-api"])).toMatchObject({
      source: expect.stringContaining("Mads::run::<AppModule>().await"),
    });
  });

  it("orders concepts by the dependencies a new reader needs", async () => {
    const repository = new FileSystemDocumentationRepository(join(process.cwd(), "content", "docs"));
    const pages = await repository.list();
    const sections = pages.reduce<string[]>((ordered, page) => {
      if (ordered.at(-1) !== page.section) {
        ordered.push(page.section);
      }
      return ordered;
    }, []);

    expect(sections).toEqual([
      "Overview",
      "Getting started",
      "Fundamentals",
      "Configuration",
      "Build an API",
      "Data and security",
      "Guides",
      "Tooling",
    ]);
    expect(pages[0]).toMatchObject({
      title: "What is MADS?",
      section: "Overview",
    });
    expect(pages.map((page) => page.slug.join("/"))).toContain("guides/clean-architecture");
  });
});
