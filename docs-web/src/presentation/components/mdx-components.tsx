"use client";

import { isValidElement, useState, type ReactNode } from "react";

function textContent(value: ReactNode): string {
  if (typeof value === "string" || typeof value === "number") {
    return String(value);
  }
  if (Array.isArray(value)) {
    return value.map(textContent).join("");
  }
  if (isValidElement<{ children?: ReactNode }>(value)) {
    return textContent(value.props.children);
  }
  return "";
}

function CodeBlock({ children }: Readonly<{ children?: ReactNode }>) {
  const [copied, setCopied] = useState(false);
  const code = textContent(children).trim();

  async function copy() {
    await navigator.clipboard?.writeText(code);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1800);
  }

  return (
    <pre className="code-block">
      <button type="button" onClick={() => void copy()} aria-label="Copy code block">{copied ? "Copied" : "Copy"}</button>
      {children}
    </pre>
  );
}

export const mdxComponents = {
  h2: ({ id, children }: Readonly<{ id?: string; children?: ReactNode }>) => <h2 id={id}>{children}</h2>,
  h3: ({ id, children }: Readonly<{ id?: string; children?: ReactNode }>) => <h3 id={id}>{children}</h3>,
  pre: CodeBlock,
  code: ({ children }: Readonly<{ children?: ReactNode }>) => <code>{children}</code>,
  a: ({ href, children }: Readonly<{ href?: string; children?: ReactNode }>) => (
    <a href={href} target={href?.startsWith("http") ? "_blank" : undefined} rel={href?.startsWith("http") ? "noreferrer" : undefined}>
      {children}
    </a>
  ),
  Callout: ({ children }: Readonly<{ children?: ReactNode }>) => <aside className="callout">{children}</aside>,
};
