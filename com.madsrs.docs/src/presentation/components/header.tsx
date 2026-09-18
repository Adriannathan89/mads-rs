import Image from "next/image";
import Link from "next/link";

import type { SearchEntry } from "@/domain/documentation";

import { SearchDialog } from "./search-dialog";
import { ThemeToggle } from "./theme-toggle";

export function Header({ searchEntries }: Readonly<{ searchEntries: readonly SearchEntry[] }>) {
  return (
    <header className="site-header">
      <Link href="/" className="brand" aria-label="MADS documentation home">
        <Image src="/mads.png" alt="" width={30} height={30} priority />
        <span>MADS</span>
        <span className="brand-muted">Documentation</span>
      </Link>
      <div className="header-actions">
        <SearchDialog entries={searchEntries} />
        <a href="https://github.com/mads-rs/mads" rel="noreferrer">GitHub</a>
        <ThemeToggle />
      </div>
    </header>
  );
}
