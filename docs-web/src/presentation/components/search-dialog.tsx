"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/navigation";

import type { SearchEntry } from "@/domain/documentation";

export function SearchDialog({ entries }: Readonly<{ entries: readonly SearchEntry[] }>) {
  const router = useRouter();
  const input = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const results = useMemo(() => {
    const term = query.trim().toLowerCase();
    return term
      ? entries.filter((entry) => `${entry.title} ${entry.description} ${entry.matchedHeading ?? ""}`.toLowerCase().includes(term))
      : entries.slice(0, 8);
  }, [entries, query]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setOpen(true);
      }
      if (event.key === "Escape") {
        setOpen(false);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  useEffect(() => {
    if (open) {
      input.current?.focus();
    }
  }, [open]);

  function choose(index: number) {
    const entry = results[index];
    if (!entry) {
      return;
    }
    router.push(`/docs/${entry.slug.join("/")}`);
    setOpen(false);
  }

  function onInputKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelected((current) => Math.min(current + 1, Math.max(results.length - 1, 0)));
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelected((current) => Math.max(current - 1, 0));
    }
    if (event.key === "Enter") {
      event.preventDefault();
      choose(selected);
    }
  }

  return (
    <>
      <button className="search-trigger" type="button" onClick={() => setOpen(true)} aria-label="Search documentation">
        Search <kbd>⌘K</kbd>
      </button>
      {open ? (
        <div className="dialog-backdrop" role="presentation" onMouseDown={() => setOpen(false)}>
          <section className="search-dialog" role="dialog" aria-modal="true" aria-label="Search documentation" onMouseDown={(event) => event.stopPropagation()}>
            <input
              ref={input}
              role="searchbox"
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                setSelected(0);
              }}
              onKeyDown={onInputKeyDown}
              placeholder="Search documentation"
              aria-controls="search-results"
              aria-activedescendant={results[selected] ? `search-result-${selected}` : undefined}
            />
            <ul id="search-results" role="listbox" aria-label="Search results">
              {results.map((entry, index) => (
                <li
                  id={`search-result-${index}`}
                  key={entry.slug.join("/")}
                  role="option"
                  aria-selected={selected === index}
                  onMouseEnter={() => setSelected(index)}
                  onClick={() => choose(index)}
                >
                  <strong>{entry.title}</strong>
                  <span>{entry.description}</span>
                </li>
              ))}
              {results.length === 0 ? <li className="search-empty">No matching pages.</li> : null}
            </ul>
          </section>
        </div>
      ) : null}
    </>
  );
}
