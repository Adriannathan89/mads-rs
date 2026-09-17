import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { FileSystemDocumentationRepository } from "@/infrastructure/content/file-system-documentation-repository";

const temporaryDirectories: string[] = [];

async function createContentDirectory(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), "mads-docs-content-"));
  temporaryDirectories.push(directory);
  await writeFile(
    join(directory, "what-is-mads.mdx"),
    `---
title: What is MADS?
description: Build modular Rust services.
section: Introduction
order: 1
---

## Why MADS?

MADS is a Rust framework for modular services.

### Composition first

Modules make application boundaries explicit.
`,
  );
  await writeFile(
    join(directory, "installation.mdx"),
    `---
title: Installation
description: Install the MADS CLI.
section: Introduction
order: 2
---

## Install MADS

Use Cargo to install the CLI.
`,
  );
  return directory;
}

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((directory) => rm(directory, { recursive: true })));
});

describe("FileSystemDocumentationRepository", () => {
  it("lists pages in section and order, then resolves a slug", async () => {
    const contentDirectory = await createContentDirectory();
    const repository = new FileSystemDocumentationRepository(contentDirectory);

    await expect(repository.list()).resolves.toMatchObject([
      { slug: ["what-is-mads"], title: "What is MADS?", order: 1 },
      { slug: ["installation"], title: "Installation", order: 2 },
    ]);
    await expect(repository.get(["what-is-mads"])).resolves.toMatchObject({
      description: expect.stringContaining("Rust"),
      headings: [
        { id: "why-mads", text: "Why MADS?", depth: 2 },
        { id: "composition-first", text: "Composition first", depth: 3 },
      ],
    });
  });

  it("does not resolve a path outside the content directory", async () => {
    const contentDirectory = await createContentDirectory();
    const repository = new FileSystemDocumentationRepository(contentDirectory);

    await expect(repository.get(["..", "Cargo"])).resolves.toBeNull();
  });

  it("searches titles, descriptions, headings, and page text", async () => {
    const contentDirectory = await createContentDirectory();
    const repository = new FileSystemDocumentationRepository(contentDirectory);

    await expect(repository.search("composition")).resolves.toEqual([
      expect.objectContaining({
        title: "What is MADS?",
        slug: ["what-is-mads"],
        matchedHeading: "Composition first",
      }),
    ]);
  });

  it("loads the shipped documentation catalog", async () => {
    const repository = new FileSystemDocumentationRepository(
      join(process.cwd(), "content", "docs"),
    );

    await expect(repository.list()).resolves.toEqual(
      expect.arrayContaining([
        expect.objectContaining({ slug: ["introduction", "quick-start"], title: "Quick start" }),
        expect.objectContaining({ slug: ["guides", "build-a-user-api"], title: "Build a user API" }),
      ]),
    );
  });
});
