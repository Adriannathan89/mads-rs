import type { MetadataRoute } from "next";
import { join } from "node:path";

import { FileSystemDocumentationRepository } from "@/infrastructure/content/file-system-documentation-repository";

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
  const pages = await new FileSystemDocumentationRepository(join(process.cwd(), "content", "docs")).list();
  return [
    { url: "https://docs.madsrs.com", changeFrequency: "weekly", priority: 1 },
    ...pages.map((page) => ({
      url: `https://docs.madsrs.com/docs/${page.slug.join("/")}`,
      changeFrequency: "monthly" as const,
      priority: 0.8,
    })),
  ];
}
