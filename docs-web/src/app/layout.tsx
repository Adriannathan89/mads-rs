import type { Metadata } from "next";
import type { ReactNode } from "react";

import { VisitorTracker } from "@/presentation/components/visitor-tracker";

import "./globals.css";

export const metadata: Metadata = {
  title: {
    default: "MADS documentation",
    template: "%s | MADS documentation",
  },
  description: "Documentation for building modular Rust services with MADS.",
  metadataBase: new URL("https://docs.madsrs.com"),
};

export default function RootLayout({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <html lang="en">
      <body>
        {children}
        <VisitorTracker />
      </body>
    </html>
  );
}
